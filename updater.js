// In-app auto-update for the formats the OS / package manager does not update
// on its own: Windows (nsis + portable), macOS (dmg) and the Linux AppImage.
//
// deb and rpm installs are deliberately skipped — they update through apt/dnf
// against the repos the postinstall scripts register (build/deb-postinstall,
// build/rpm-postinstall). The update feed is the `publish` block in
// package.json (the GitHub release), which is where electron-builder writes the
// latest*.yml metadata electron-updater reads.

const { app } = require('electron');

// Whether an in-app update check makes sense for how this copy was installed.
function updatesSupported() {
  // No published feed to check against in a dev run.
  if (!app.isPackaged) return false;

  // On Linux only the AppImage can self-update; deb/rpm defer to the system
  // package manager. electron-builder sets APPIMAGE when running as one.
  if (process.platform === 'linux') return Boolean(process.env.APPIMAGE);

  // Windows and macOS always.
  return true;
}

function setupAutoUpdater() {
  if (!updatesSupported()) return;

  // electron-updater is CommonJS but heavy; require it lazily so a dev run
  // (where it is skipped) never loads it.
  let autoUpdater;
  try {
    ({ autoUpdater } = require('electron-updater'));
  } catch (err) {
    console.error('electron-updater unavailable:', err);
    return;
  }

  autoUpdater.on('error', (err) => console.error('auto-update error:', err));

  // Downloads a newer version in the background and installs it on next launch.
  // macOS applies updates only for a signed + notarized app, so until signing
  // secrets are configured this logs an error and no-ops rather than updating.
  autoUpdater.checkForUpdatesAndNotify().catch((err) => {
    console.error('checkForUpdatesAndNotify failed:', err);
  });
}

module.exports = { setupAutoUpdater };
