#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;
use std::{fs, path::PathBuf, process::Command};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

// كل البيانات تُحفظ في مجلد data بجانب الـ exe
fn base_dir() -> PathBuf {
    let d = std::env::current_exe().ok().and_then(|p| p.parent().map(|x| x.to_path_buf())).unwrap_or_else(|| PathBuf::from("."));
    let d = d.join("data");
    let _ = fs::create_dir_all(d.join("covers"));
    d
}
fn quiet(c: &mut Command) {
    #[cfg(windows)]
    c.creation_flags(0x0800_0000);
    let _ = c;
}
#[tauri::command]
fn load_data() -> String { fs::read_to_string(base_dir().join("launcher-data.json")).unwrap_or_default() }
#[tauri::command]
fn save_data(json: String) -> Result<(), String> { fs::write(base_dir().join("launcher-data.json"), json).map_err(|e| e.to_string()) }

#[tauri::command]
async fn list_drives() -> String {
    let ps = "[Console]::OutputEncoding=[Text.Encoding]::UTF8;$d=Get-Disk|%{$n=$_.Number;$f=$_.FriendlyName;Get-Partition -DiskNumber $n -ErrorAction SilentlyContinue|?{$_.DriveLetter}|%{[pscustomobject]@{disk=$n;name=$f;letter=[string]$_.DriveLetter;size=$_.Size}}};if($d){@($d)|ConvertTo-Json -Compress}else{'[]'}";
    tauri::async_runtime::spawn_blocking(move || {
        let mut c = Command::new("powershell");
        quiet(&mut c);
        c.args(["-NoProfile", "-NonInteractive", "-Command", ps]);
        match c.output() { Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(), Err(_) => "[]".to_string() }
    }).await.unwrap_or_else(|_| "[]".to_string())
}
#[tauri::command]
async fn pick_folder() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().pick_folder()).await.ok().flatten().map(|p| p.to_string_lossy().to_string())
}
#[tauri::command]
async fn pick_file() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().add_filter("Programs", &["exe", "lnk", "bat"]).pick_file()).await.ok().flatten().map(|p| p.to_string_lossy().to_string())
}
#[tauri::command]
fn scan(path: String) -> Result<Vec<String>, String> {
    let mut v: Vec<String> = fs::read_dir(&path).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok()).filter(|e| e.path().is_dir())
        .map(|e| e.path().to_string_lossy().to_string()).collect();
    v.sort();
    Ok(v)
}
#[tauri::command]
fn launch(path: String) -> Result<(), String> {
    let dir = PathBuf::from(&path).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let mut c = Command::new("cmd");
    quiet(&mut c);
    c.args(["/C", "start", "", "/D", dir.as_str(), path.as_str()]);
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}
#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    Command::new("explorer.exe").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}
fn curl(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut c = Command::new("curl.exe");
    quiet(&mut c);
    c.args(args);
    let o = c.output().map_err(|e| format!("curl.exe غير موجود: {}", e))?;
    if !o.status.success() { return Err("فشل الاتصال بـ SteamGridDB".to_string()); }
    Ok(o.stdout)
}
fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{:02X}", b) }).collect()
}
#[tauri::command]
async fn fetch_cover(name: String, key: String, dim: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let auth = format!("Authorization: Bearer {}", key);
        let u1 = format!("https://www.steamgriddb.com/api/v2/search/autocomplete/{}", enc(&name));
        let j: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u1.as_str()])?).map_err(|_| "رد غير صالح (تأكد من المفتاح)".to_string())?;
        let id = j["data"][0]["id"].as_i64().ok_or("لا نتائج لهذا الاسم")?;
        let u2 = format!("https://www.steamgriddb.com/api/v2/grids/game/{}?dimensions={}", id, dim);
        let g: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u2.as_str()])?).map_err(|_| "رد غير صالح".to_string())?;
        let url = g["data"][0]["url"].as_str().ok_or("لا يوجد بوستر")?.to_string();
        let file = base_dir().join("covers").join(format!("{}.img", id));
        let fs_ = file.to_string_lossy().to_string();
        curl(&["-s", "-L", "-o", fs_.as_str(), url.as_str()])?;
        Ok(fs_)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
fn read_cover(path: String) -> Result<String, String> {
    let b = fs::read(&path).map_err(|e| e.to_string())?;
    let mime = if b.starts_with(&[0x89, 0x50]) { "image/png" } else if b.starts_with(&[0xFF, 0xD8]) { "image/jpeg" } else if b.len() > 12 && &b[8..12] == b"WEBP" { "image/webp" } else { "image/png" };
    Ok(format!("data:{};base64,{}", mime, STANDARD.encode(b)))
}
#[tauri::command]
fn quit(app: tauri::AppHandle) { app.exit(0); }

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![load_data, save_data, list_drives, pick_folder, pick_file, scan, launch, open_path, fetch_cover, read_cover, quit])
        .run(tauri::generate_context!())
        .expect("error while running app");
}
