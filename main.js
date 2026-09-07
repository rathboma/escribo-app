const { app, BrowserWindow, Tray, Menu, ipcMain, clipboard, dialog } = require('electron');
const fs = require('fs');
const path = require('path');

const APP_NAME = 'escribo';

app.setName(APP_NAME);

let tray = null;
const watchedDir = path.join(app.getPath('pictures'), 'Screenshots');

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
      contextIsolation: true
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

app.whenReady().then(() => {
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
