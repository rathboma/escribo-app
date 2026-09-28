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
  // The license on this device. Checked in the main process, which knows the
  // device code; see src/license.js.
  license: {
    status: () => ipcRenderer.invoke('license-status'),
    activate: (code) => ipcRenderer.invoke('license-activate', code),
    remove: () => ipcRenderer.invoke('license-remove'),
    openActivationPage: () => ipcRenderer.invoke('license-open-activation')
  }
});
