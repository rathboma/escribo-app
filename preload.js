const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  platform: process.platform,
  getFileContent: (filePath) => ipcRenderer.invoke('get-file-content', filePath),
  saveImage: (dataUrl, suggestedName) => ipcRenderer.invoke('save-image', dataUrl, suggestedName),
  saveText: (text, suggestedName) => ipcRenderer.invoke('save-text', text, suggestedName),
  openText: () => ipcRenderer.invoke('open-text'),
  window: (action) => ipcRenderer.send('window-control', action)
});
