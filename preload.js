const { contextBridge, ipcRenderer: r } = require('electron');
contextBridge.exposeInMainWorld('api', {
  load: () => r.invoke('load'), save: d => r.invoke('save', d),
  drives: () => r.invoke('drives'), pickFolder: () => r.invoke('pickFolder'),
  pickExe: () => r.invoke('pickExe'), scan: p => r.invoke('scan', p),
  launch: p => r.invoke('launch', p), open: p => r.invoke('open', p),
  sgdb: (n, k, d) => r.invoke('sgdb', n, k, d)
});
