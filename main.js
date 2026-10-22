const { app, BrowserWindow, Tray, Menu, ipcMain, clipboard, dialog } = require('electron');
const fs = require('fs');
const path = require('path');
const { setupAutoUpdater } = require('./updater');

const APP_NAME = 'escribo';

app.setName(APP_NAME);

let tray = null;
const watchedDir = path.join(app.getPath('pictures'), 'Screenshots');

// Persistent settings live in a JSON file under userData, not in the
// renderer's localStorage — which the OS is free to clear at any time.
let store = null;
// Settings the main process owns. Kept in their own file because the renderer
// replaces the whole of `store` on every save.
let appStore = null;

const IMAGE_EXTENSIONS = ['.png', '.jpg', '.jpeg', '.gif', '.svg'];
const RECENT_COUNT = 5;
let recentScreenshots = [];

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
  tray.setToolTip(APP_NAME);
  updateTrayMenu();
}

// Rebuilt whenever the recent list or a setting changes — Linux only picks up
// a new menu through setContextMenu, not by mutating the old one.
function updateTrayMenu() {
  if (!tray) return;
  const recent = recentScreenshots.length
    ? recentScreenshots.map((filePath) => ({
      // Windows reads '&' as a mnemonic marker.
      label: path.basename(filePath).replace(/&/g, '&&'),
      click: () => { createFileWindow(filePath); }
    }))
    : [{ label: 'No screenshots yet', enabled: false }];

  const contextMenu = Menu.buildFromTemplate([
    { label: 'Recent screenshots', enabled: false },
    ...recent,
    { type: 'separator' },
    { label: 'New window', click: () => { createFileWindow(); } },
    { label: 'Choose file', click: () => { triggerFromDialog(); } },
    { label: 'Paste from clipboard', click: () => { triggerFromClipboard(); } },
    { type: 'separator' },
    {
      label: 'Open new screenshots automatically',
      type: 'checkbox',
      checked: appStore.get('openOnScreenshot'),
      click: (item) => {
        appStore.set('openOnScreenshot', item.checked);
        updateTrayMenu();
      }
    },
    { type: 'separator' },
    { label: 'Quit', click: () => { app.quit(); } }
  ]);
  tray.setContextMenu(contextMenu);
}

function isImageFile(filename) {
  return !filename.startsWith('.') &&
    IMAGE_EXTENSIONS.includes(path.extname(filename).toLowerCase());
}

// The newest images in the screenshot folder, newest first.
async function refreshRecentScreenshots() {
  try {
    const names = (await fs.promises.readdir(watchedDir)).filter(isImageFile);
    const files = await Promise.all(names.map(async (name) => {
      const filePath = path.join(watchedDir, name);
      try {
        const stat = await fs.promises.stat(filePath);
        return stat.isFile() ? { filePath, mtime: stat.mtimeMs } : null;
      } catch {
        return null; // Removed between readdir and stat.
      }
    }));
    recentScreenshots = files
      .filter(Boolean)
      .sort((a, b) => b.mtime - a.mtime)
      .slice(0, RECENT_COUNT)
      .map((f) => f.filePath);
  } catch (error) {
    console.error(`Could not list ${watchedDir}:`, error.message);
    recentScreenshots = [];
  }
  updateTrayMenu();
}

function watchScreenshotDir() {
  try {
    fs.mkdirSync(watchedDir, { recursive: true });
  } catch (error) {
    console.error(`Could not create ${watchedDir}:`, error.message);
    return;
  }

  refreshRecentScreenshots();

  // A single screenshot can fire several events while it is written.
  let refreshTimer = null;

  try {
    fs.watch(watchedDir, (eventType, filename) => {
      if (eventType !== 'rename' || !filename) return;
      clearTimeout(refreshTimer);
      refreshTimer = setTimeout(refreshRecentScreenshots, 300);

      const filePath = path.join(watchedDir, filename);
      // 'rename' fires on both addition and removal — only react to additions.
      if (isImageFile(filename) && fs.existsSync(filePath)) {
        console.log(`New file detected: ${filePath}`);
        if (appStore.get('openOnScreenshot')) createFileWindow(filePath);
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
  appStore = new Store({ name: 'app', defaults: { openOnScreenshot: true } });

  // The renderer keeps its settings load/save synchronous, so serve these
  // over the synchronous IPC channel.
  ipcMain.on('settings-get', (event) => {
    event.returnValue = store.store;
  });
  ipcMain.on('settings-set', (event, data) => {
    store.store = data;
    event.returnValue = true;
  });

  // macOS takes the dock icon from the app bundle and ignores BrowserWindow's
  // `icon`, so an unpackaged run would show the stock Electron icon.
  if (process.platform === 'darwin' && app.dock) {
    app.dock.setIcon(path.join(__dirname, 'assets', 'icon.png'));
  }

  createTray();
  watchScreenshotDir();

  // Check for updates in the background (AppImage / Windows / macOS only).
  setupAutoUpdater();

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
  // Stay resident in the tray so the next screenshot can open a window, or be
  // picked from the tray's recent list.
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
