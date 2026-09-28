// Keeps escribo up to date.
//
// Three builds can replace themselves: the macOS app, the Windows installer
// and the Linux AppImage. Those download a newer release in the background and
// install it when escribo restarts, or the next time it quits. The rest — the
// Windows portable exe, deb, rpm and Flatpak — have nothing that could swap
// them out, so they are only told that a new version exists and where to get
// it. So is the Mac app while its builds are unsigned: macOS refuses to install
// an update into an app that isn't, and that failure lands on the same page.
//
// The feed is the GitHub release that `build.publish` in package.json names.
// electron-builder bakes it into every build as resources/app-update.yml and
// writes the latest*.yml files this reads next to the installers, so a release
// has to carry those files for anyone to be offered it.
//
// Nothing else in escribo goes online, and the automatic check can be switched
// off in Preferences → Updates.

const { app, BrowserWindow, dialog, ipcMain, shell } = require('electron');
const { AppImageUpdater, AppUpdater, MacUpdater, NsisUpdater } = require('electron-updater');
const fs = require('fs');
const path = require('path');

const FIRST_CHECK_DELAY = 10 * 1000;         // let startup finish first
const CHECK_INTERVAL = 6 * 60 * 60 * 1000;   // then every six hours
const DOWNLOAD_PAGE = 'https://escriboapp.com/get/';

// electron-updater stamps each request for the feed with a random id it keeps
// in userData (x-user-staging-id), for staged rollouts on a server of your
// own. GitHub has no use for it, and it would tell one install's requests
// apart from another's, so every copy sends the same nil UUID instead.
const REQUEST_HEADERS = { 'x-user-staging-id': '00000000-0000-0000-0000-000000000000' };

/** A build that reads the same feed but has no way to install what it finds. */
class NoticeOnlyUpdater extends AppUpdater {
  doDownloadUpdate() {
    return Promise.reject(new Error('This build of escribo cannot update itself.'));
  }

  quitAndInstall() {}
}

let updater = null;
let store = null;
let timer = null;
let onChange = () => {};

// Everything the tray and the windows show. `status` is one of:
//   idle         nothing checked yet
//   checking     asking the feed
//   latest       this is the newest version
//   downloading  fetching `next` in the background
//   ready        `next` is downloaded and installs on restart
//   available    `next` is out, but has to be downloaded by hand: this build
//                can't replace itself, or installing it went wrong (`error`)
//   error        the check itself failed (`error`)
const state = {
  version: app.getVersion(),
  build: 'dev',
  selfUpdate: false,
  auto: true,
  status: 'idle',
  next: null,
  percent: null,
  error: null
};

/** How this copy was installed.
 *
 *  electron-updater would work the Linux case out itself, from a package-type
 *  file the deb and rpm builds write to resources/. But they write it into the
 *  folder every Linux target is packed from, so whatever is packed after them
 *  inherits it: the Flatpak does, and an AppImage built after either package
 *  would, and would then try to update itself with dpkg or rpm. The AppImage
 *  is recognised by its runtime instead. */
function installKind() {
  if (!app.isPackaged) return 'dev';
  if (process.platform === 'darwin') return 'mac';
  if (process.platform === 'win32') {
    // Set by the portable exe's launcher; the installed app never has it.
    return process.env.PORTABLE_EXECUTABLE_FILE ? 'portable' : 'nsis';
  }
  // Set by the AppImage runtime to the path of the .AppImage being run.
  if (process.env.APPIMAGE) return 'appimage';
  if (process.env.FLATPAK_ID || fs.existsSync('/.flatpak-info')) return 'flatpak';
  return 'package';   // deb or rpm
}

function writable(dir) {
  try {
    fs.accessSync(dir, fs.constants.W_OK);
    return true;
  } catch {
    return false;
  }
}

/** Whether this copy can replace itself where it is. The AppImage updater
 *  writes the new file next to the running one and deletes the old; macOS
 *  swaps the whole bundle, which fails from a disk image, a translocated
 *  download or an /Applications the user can't write to. */
function canSelfUpdate(build) {
  if (build === 'nsis') return true;
  if (build === 'appimage') return writable(path.dirname(process.env.APPIMAGE));
  if (build === 'mac') return writable(path.resolve(process.execPath, '../../../..'));
  return false;
}

function createUpdater(build, selfUpdate) {
  if (!selfUpdate) return new NoticeOnlyUpdater();
  // Chosen here rather than through electron-updater's `autoUpdater`, which
  // trusts package-type (see installKind).
  if (build === 'mac') return new MacUpdater();
  if (build === 'nsis') return new NsisUpdater();
  return new AppImageUpdater();
}

function downloadPage() {
  const os = { darwin: 'macos', win32: 'windows' }[process.platform] || 'linux';
  return `${DOWNLOAD_PAGE}?os=${os}`;
}

/** electron-updater's messages can run to a stack trace or a page of XML. */
function describeError(err) {
  const message = String((err && err.message) || err || '');
  if (/net::ERR_|ENOTFOUND|ECONNREFUSED|ETIMEDOUT|EAI_AGAIN/.test(message)) {
    return 'escribo couldn’t reach GitHub.';
  }
  const line = message.split('\n')[0].trim();
  return line.length > 160 ? `${line.slice(0, 157)}…` : line || 'Something went wrong.';
}

function snapshot() {
  return { ...state };
}

function set(changes) {
  Object.assign(state, changes);
  const s = snapshot();
  BrowserWindow.getAllWindows().forEach((win) => win.webContents.send('updates-changed', s));
  onChange(s);
}

function check() {
  if (!updater || ['checking', 'downloading', 'ready'].includes(state.status)) return;
  // Failures, the background download's included, arrive as 'error' events.
  updater.checkForUpdates()
    .then((result) => result && result.downloadPromise && result.downloadPromise.catch(() => {}))
    .catch(() => {});
}

function stopTimer() {
  clearTimeout(timer);
  timer = null;
}

function startTimer() {
  stopTimer();
  if (!updater || !state.auto || state.status === 'ready') return;
  timer = setTimeout(function tick() {
    check();
    timer = setTimeout(tick, CHECK_INTERVAL);
  }, FIRST_CHECK_DELAY);
}

function setAuto(on) {
  state.auto = !!on;
  store.set('auto', state.auto);
  startTimer();
  set({});
}

/** Restart into a downloaded update, or send people to the download page for
 *  one this copy can't install. */
async function apply(win) {
  if (state.status === 'available') {
    shell.openExternal(downloadPage()).catch((err) => console.error('Could not open the download page:', err));
    return;
  }
  if (state.status !== 'ready') return;

  // A restart closes every window, and with them any unsaved annotations.
  if (BrowserWindow.getAllWindows().length) {
    const options = {
      type: 'question',
      buttons: ['Restart', 'Later'],
      defaultId: 0,
      cancelId: 1,
      message: `Restart escribo to update to ${state.next}?`,
      detail: 'Every escribo window will close. Copy or save anything you want to keep first.'
    };
    const { response } = await (win ? dialog.showMessageBox(win, options) : dialog.showMessageBox(options));
    if (response !== 0) return;
  }
  // Silently, then start the new version: the installer on Windows would
  // otherwise show its own window, and the AppImage would not come back up.
  updater.quitAndInstall(true, true);
}

/** The tray's line for a waiting update, or null when there is none. */
function menuItem() {
  if (state.status === 'ready') {
    return { label: `Restart to update to ${state.next}`, click: () => apply() };
  }
  if (state.status === 'available') {
    return { label: `Download escribo ${state.next}`, click: () => apply() };
  }
  return null;
}

function wireEvents() {
  updater.on('checking-for-update', () => set({ status: 'checking', error: null }));
  updater.on('update-not-available', () => set({ status: 'latest', next: null }));
  updater.on('update-available', (info) => set({
    status: state.selfUpdate ? 'downloading' : 'available',
    next: info.version,
    percent: state.selfUpdate ? 0 : null
  }));
  updater.on('download-progress', (progress) => set({ percent: Math.round(progress.percent) }));
  updater.on('update-downloaded', (info) => {
    // Nothing more to look for until it is installed.
    stopTimer();
    set({ status: 'ready', next: info.version, percent: null });
  });
  updater.on('appimage-filename-updated', (file) => console.log(`The AppImage is now ${file}`));
  updater.on('error', (err) => {
    console.error('Update failed:', err);
    // Once a newer version has been found, a failure to fetch or install it,
    // or to check again later, still leaves the download page to get it from.
    set({ status: state.next ? 'available' : 'error', percent: null, error: describeError(err) });
  });
}

/** Start checking for updates. `prefs` is the electron-store that remembers
 *  whether the automatic checks are on; `listener` hears every change. */
function start(prefs, listener) {
  store = prefs;
  onChange = listener || onChange;
  state.auto = store.get('auto', true) !== false;
  state.build = installKind();
  state.selfUpdate = canSelfUpdate(state.build);

  ipcMain.handle('updates-state', () => snapshot());
  ipcMain.handle('updates-check', () => { check(); return snapshot(); });
  ipcMain.handle('updates-set-auto', (event, on) => { setAuto(on); return snapshot(); });
  ipcMain.on('updates-apply', (event) => { apply(BrowserWindow.fromWebContents(event.sender)); });

  // A development run has no feed to read.
  if (state.build === 'dev') return;

  // However updates break, the app itself has to keep working.
  try {
    updater = createUpdater(state.build, state.selfUpdate);
  } catch (err) {
    console.error('Updates are unavailable:', err);
    set({ status: 'error', error: describeError(err) });
    return;
  }
  updater.autoDownload = state.selfUpdate;
  updater.requestHeaders = REQUEST_HEADERS;
  wireEvents();
  startTimer();
}

module.exports = { start, menuItem };
