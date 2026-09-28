const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  platform: process.platform,
  getFileContent: (filePath) => ipcRenderer.invoke('get-file-content', filePath),
  saveImage: (dataUrl, suggestedName) => ipcRenderer.invoke('save-image', dataUrl, suggestedName),
  saveText: (text, suggestedName) => ipcRenderer.invoke('save-text', text, suggestedName),
  openText: () => ipcRenderer.invoke('open-text'),
  window: (action) => ipcRenderer.send('window-control', action),
  // Persisted settings, backed by electron-store in the main process.
  getSettings: () => ipcRenderer.sendSync('settings-get'),
  saveSettings: (data) => ipcRenderer.sendSync('settings-set', data),
  // Updates are checked for and installed by the main process (updater.js);
  // windows show its state and pass on the clicks.
  updates: {
    state: () => ipcRenderer.invoke('updates-state'),
    check: () => ipcRenderer.invoke('updates-check'),
    setAuto: (on) => ipcRenderer.invoke('updates-set-auto', on),
    apply: () => ipcRenderer.send('updates-apply'),
    onChange: (callback) => ipcRenderer.on('updates-changed', (event, state) => callback(state))
  }
});
