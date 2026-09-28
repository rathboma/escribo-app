const { app, BrowserWindow, Tray, Menu, ipcMain, clipboard, dialog, shell } = require('electron');
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const license = require('./src/license.js');

const APP_NAME = 'escribo';

app.setName(APP_NAME);

let tray = null;
const watchedDir = path.join(app.getPath('pictures'), 'Screenshots');

// Persistent settings live in a JSON file under userData, not in the
// renderer's localStorage — which the OS is free to clear at any time.
let store = null;

// The activation code lives in a file of its own, so restoring the default
// settings, or importing a profile, can never take a license with it.
let licenseStore = null;
let deviceCode = null;

// This machine's device code, worked out once per launch. A machine whose ID
// can't be read gets a random one instead, kept with the license so the code
// stays the same from one launch to the next.
async function getDeviceCode() {
  if (deviceCode) return deviceCode;
  let id = await license.readMachineId();
  if (!id) {
    id = licenseStore.get('installId');
    if (!id) {
      id = crypto.randomUUID();
      licenseStore.set('installId', id);
    }
  }
  deviceCode = license.deviceCodeFromMachineId(id);
  return deviceCode;
}

// A license is for a major version: 1.x licenses unlock every 1.x release.
function appMajor() {
  return Number.parseInt(app.getVersion(), 10) || 1;
}

// Checked from scratch every time, so a license file copied over from
// another machine, or kept from an older major version, does nothing.
async function licenseStatus() {
  const code = await getDeviceCode();
  const saved = licenseStore.get('activation');
  const result = saved ? license.verifyActivation(saved, code, { appMajor: appMajor() }) : null;
  return {
    deviceCode: license.formatDeviceCode(code),
    activation: result && result.ok ? result.activation : null,
    problem: result && !result.ok ? result.reason : null
  };
}

// Read image content from the clipboard and create a file window.
function getImageFromClipboard() {
  const image = clipboard.readImage();
  if (image.isEmpty()) {
    console.error('Clipboard does not contain an image.');
    return;
  }
  // Convert the image to a Data URL (e.g. "data:image/png;base64,...")
  const dataURL = image.toDataURL();
  const matches = dataURL.match(/^data:(.+);base64,(.+)$/);
  if (!matches) {
    console.error('Invalid image data URL.');
    return;
  }
  const mimeType = matches[1];
  const base64Data = matches[2];
  return {
    filePath: 'app:clipboard',
    mimeType,
    content: base64Data
  };
}

function triggerFromClipboard() {
  createFileWindow('app:clipboard');
}

async function triggerFromDialog() {
  try {
    const result = await dialog.showOpenDialog({
      properties: ['openFile'],
      filters: [
        { name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'svg'] }
      ]
    });
    if (!result.canceled && result.filePaths.length > 0) {
      createFileWindow(result.filePaths[0]);
    } else {
      console.error('No file');
    }
  } catch (ex) {
    console.error('ERROR WITH DIALOG', ex);
  }
}

// Create a new window that loads index.html with a query parameter for the file path.
function createFileWindow(filePath) {
  // The renderer draws the title bar. On macOS the native traffic lights stay,
  // positioned to sit in it; elsewhere the renderer draws the window buttons.
  const chrome = process.platform === 'darwin'
    ? { titleBarStyle: 'hidden', trafficLightPosition: { x: 13, y: 13 } }
    : { frame: false };

  let win = new BrowserWindow({
    width: 1280,
    height: 830,
    minWidth: 720,
    minHeight: 520,
    ...chrome,
    show: false,
    backgroundColor: '#1f1f24',
    icon: path.join(__dirname, 'assets', 'icon.png'),
    webPreferences: {
      // Use a preload script to expose IPC safely in the renderer.
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      // The app makes no network requests of its own, and this keeps Electron
      // from making one on its behalf: with spellcheck on, it downloads
      // dictionaries from a Google CDN on Windows and Linux.
      spellcheck: false
    }
  });

  win.once('ready-to-show', () => win.show());

  const query = filePath ? `?file=${encodeURIComponent(filePath)}` : '';
  win.loadURL(`file://${__dirname}/index.html${query}`);

  win.on('closed', () => {
    win = null;
  });

  return win;
}

// Create a system tray icon with a context menu.
function createTray() {
  tray = new Tray(path.join(__dirname, 'tray-icon.png')); // Ensure this file exists.
  const contextMenu = Menu.buildFromTemplate([
    { label: 'New window', click: () => { createFileWindow(); } },
    { label: 'Choose file', click: () => { triggerFromDialog(); } },
    { label: 'Paste from clipboard', click: () => { triggerFromClipboard(); } },
    { label: 'Quit', click: () => { app.quit(); } }
  ]);
  tray.setToolTip(APP_NAME);
  tray.setContextMenu(contextMenu);
}

function watchScreenshotDir() {
  try {
    fs.mkdirSync(watchedDir, { recursive: true });
  } catch (error) {
    console.error(`Could not create ${watchedDir}:`, error.message);
    return;
  }

  try {
    fs.watch(watchedDir, (eventType, filename) => {
      if (eventType !== 'rename' || !filename) return;
      const filePath = path.join(watchedDir, filename);
      // 'rename' fires on both addition and removal — only react to additions.
      if (fs.existsSync(filePath)) {
        console.log(`New file detected: ${filePath}`);
        createFileWindow(filePath);
      }
    });
  } catch (error) {
    console.error(`Could not watch ${watchedDir}:`, error.message);
  }
}

app.whenReady().then(async () => {
  // electron-store is ESM-only, so pull it in with a dynamic import.
  const { default: Store } = await import('electron-store');
  store = new Store({ name: 'settings' });

  // The renderer keeps its settings load/save synchronous, so serve these
  // over the synchronous IPC channel.
  ipcMain.on('settings-get', (event) => {
    event.returnValue = store.store;
  });
  ipcMain.on('settings-set', (event, data) => {
    store.store = data;
    event.returnValue = true;
  });

  licenseStore = new Store({ name: 'license' });

  ipcMain.handle('license-status', () => licenseStatus());

  ipcMain.handle('license-activate', async (event, code) => {
    const result = license.verifyActivation(code, await getDeviceCode(), { appMajor: appMajor() });
    if (!result.ok) return { ok: false, reason: result.reason };
    licenseStore.set('activation', String(code).replace(/\s+/g, ''));
    return { ok: true, status: await licenseStatus() };
  });

  ipcMain.handle('license-remove', () => {
    licenseStore.delete('activation');
    return licenseStatus();
  });

  // Opens the activation page in the browser, with this device's code filled
  // in. The only way escribo ever sends anything is the person doing it.
  ipcMain.handle('license-open-activation', async () => {
    const code = license.formatDeviceCode(await getDeviceCode());
    await shell.openExternal(`${license.ACTIVATE_URL}?device=${encodeURIComponent(code)}`);
  });

  // macOS takes the dock icon from the app bundle and ignores BrowserWindow's
  // `icon`, so an unpackaged run would show the stock Electron icon.
  if (process.platform === 'darwin' && app.dock) {
    app.dock.setIcon(path.join(__dirname, 'assets', 'icon.png'));
  }

  createTray();
  watchScreenshotDir();

  // Start with a blank window — its drop area and file picker are the way in
  // until a screenshot arrives on its own.
  createFileWindow();

  // Listen for IPC requests for file content.
  ipcMain.handle('get-file-content', async (event, filePath) => {
    try {
      console.log('fetching ', filePath);
      if (filePath === 'app:clipboard') {
        const payload = getImageFromClipboard();
        if (payload) {
          console.log('responding with clipboard data', payload.filePath, payload.mimeType);
          return { success: true, ...payload };
        }
        console.error('No image in clipboard');
        return { success: false, error: 'No image in clipboard' };
      }

      const content = await fs.promises.readFile(filePath, { encoding: 'base64' });
      const ext = path.extname(filePath).slice(1); // remove the dot
      const mimeType = getMimeType(ext);
      console.log('responding with', filePath, mimeType);
      return { success: true, filePath, mimeType, content };
    } catch (error) {
      console.error(`Error reading file ${filePath}:`, error);
      return { success: false, error: error.message };
    }
  });

  // Window buttons drawn in the custom title bar.
  ipcMain.on('window-control', (event, action) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    if (!win) return;
    if (action === 'minimize') win.minimize();
    else if (action === 'maximize') win.isMaximized() ? win.unmaximize() : win.maximize();
    else if (action === 'close') win.close();
  });

  // Save the composed PNG through a native dialog.
  ipcMain.handle('save-image', async (event, dataUrl, suggestedName) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    try {
      const { canceled, filePath } = await dialog.showSaveDialog(win, {
        defaultPath: path.join(app.getPath('pictures'), suggestedName || 'escribo.png'),
        filters: [{ name: 'PNG image', extensions: ['png'] }]
      });
      if (canceled || !filePath) return { success: false, canceled: true };

      const base64 = String(dataUrl).replace(/^data:image\/\w+;base64,/, '');
      await fs.promises.writeFile(filePath, base64, 'base64');
      return { success: true, filePath };
    } catch (error) {
      console.error('Error saving image:', error);
      return { success: false, error: error.message };
    }
  });

  // Preset files: plain JSON through the native dialogs.
  ipcMain.handle('save-text', async (event, text, suggestedName) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    try {
      const { canceled, filePath } = await dialog.showSaveDialog(win, {
        defaultPath: path.join(app.getPath('documents'), suggestedName || 'escribo-presets.json'),
        filters: [{ name: 'JSON', extensions: ['json'] }]
      });
      if (canceled || !filePath) return { success: false, canceled: true };
      await fs.promises.writeFile(filePath, String(text), 'utf8');
      return { success: true, filePath };
    } catch (error) {
      console.error('Error saving file:', error);
      return { success: false, error: error.message };
    }
  });

  ipcMain.handle('open-text', async (event) => {
    const win = BrowserWindow.fromWebContents(event.sender);
    try {
      const { canceled, filePaths } = await dialog.showOpenDialog(win, {
        properties: ['openFile'],
        filters: [{ name: 'JSON', extensions: ['json'] }]
      });
      if (canceled || !filePaths.length) return { success: false, canceled: true };
      const text = await fs.promises.readFile(filePaths[0], 'utf8');
      return { success: true, filePath: filePaths[0], text };
    } catch (error) {
      console.error('Error reading file:', error);
      return { success: false, error: error.message };
    }
  });
});

app.on('window-all-closed', () => {
  // Stay resident in the tray so the next screenshot can open a window.
});

app.on('activate', () => {
  // macOS: clicking the dock icon with no windows open should give one.
  if (BrowserWindow.getAllWindows().length === 0) createFileWindow();
});

function getMimeType(ext) {
  switch (ext.toLowerCase()) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'svg':
      return 'image/svg+xml';
    default:
      return 'application/octet-stream';
  }
}
