const { app, BrowserWindow, ipcMain, dialog, shell } = require('electron');
const path = require('path'), fs = require('fs'), cp = require('child_process');
const base = process.env.PORTABLE_EXECUTABLE_DIR || app.getPath('userData');
const dataFile = path.join(base, 'launcher-data.json'), covers = path.join(base, 'covers');
fs.mkdirSync(covers, { recursive: true });
let win;
app.whenReady().then(() => {
  win = new BrowserWindow({ width: 1280, height: 720, minWidth: 900, minHeight: 560, autoHideMenuBar: true,
    backgroundColor: '#1b1b1f', webPreferences: { preload: path.join(__dirname, 'preload.js'), webSecurity: false } });
  win.loadFile('index.html');
});
app.on('window-all-closed', () => app.quit());
ipcMain.handle('load', () => { try { return JSON.parse(fs.readFileSync(dataFile, 'utf8')); } catch { return null; } });
ipcMain.handle('save', (_, d) => { fs.writeFileSync(dataFile, JSON.stringify(d, null, 1)); return true; });
ipcMain.handle('drives', () => new Promise(res => {
  const ps = "$d=Get-Disk|%{$n=$_.Number;$f=$_.FriendlyName;Get-Partition -DiskNumber $n|?{$_.DriveLetter}|%{[pscustomobject]@{disk=$n;name=$f;letter=[string]$_.DriveLetter;size=$_.Size}}};$d|ConvertTo-Json -Compress";
  cp.execFile('powershell', ['-NoProfile', '-Command', ps], { maxBuffer: 1e7 }, (e, o) => {
    try { const j = JSON.parse(o); res(Array.isArray(j) ? j : [j]); } catch { res([]); }
  });
}));
ipcMain.handle('pickFolder', async () => { const r = await dialog.showOpenDialog(win, { properties: ['openDirectory'] }); return r.canceled ? null : r.filePaths[0]; });
ipcMain.handle('pickExe', async () => { const r = await dialog.showOpenDialog(win, { filters: [{ name: 'Programs', extensions: ['exe', 'lnk', 'bat'] }], properties: ['openFile'] }); return r.canceled ? null : r.filePaths[0]; });
ipcMain.handle('scan', (_, p) => fs.readdirSync(p, { withFileTypes: true }).filter(d => d.isDirectory()).map(d => path.join(p, d.name)));
ipcMain.handle('launch', (_, p) => { cp.spawn(p, [], { cwd: path.dirname(p), detached: true, stdio: 'ignore' }).unref(); return true; });
ipcMain.handle('open', (_, p) => shell.openPath(p));
ipcMain.handle('sgdb', async (_, name, key, dim) => {
  try {
    const h = { Authorization: 'Bearer ' + key };
    const s = await (await fetch('https://www.steamgriddb.com/api/v2/search/autocomplete/' + encodeURIComponent(name), { headers: h })).json();
    if (!s.data || !s.data.length) return { error: 'لا نتائج' };
    const g = await (await fetch(`https://www.steamgriddb.com/api/v2/grids/game/${s.data[0].id}?dimensions=${dim || '600x900'}`, { headers: h })).json();
    if (!g.data || !g.data.length) return { error: 'لا يوجد بوستر' };
    const buf = Buffer.from(await (await fetch(g.data[0].url)).arrayBuffer());
    const f = path.join(covers, s.data[0].id + '.png'); fs.writeFileSync(f, buf);
    return { file: f };
  } catch (e) { return { error: String(e.message || e) }; }
});
