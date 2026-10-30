#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
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
async fn pick_file(kind: String, dir: Option<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = rfd::FileDialog::new();
        if let Some(p) = dir { d = d.set_directory(p); }
        d = if kind == "image" { d.add_filter("Images", &["png", "jpg", "jpeg", "webp"]) } else { d.add_filter("Programs", &["exe", "lnk", "bat"]) };
        d.pick_file()
    }).await.ok().flatten().map(|p| p.to_string_lossy().to_string())
}
#[tauri::command]
async fn export_data(json: String) -> Result<(), String> {
    let p = tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().set_file_name("launcher-backup.json").save_file()).await.map_err(|e| e.to_string())?;
    match p { Some(p) => fs::write(p, json).map_err(|e| e.to_string()), None => Ok(()) }
}
#[tauri::command]
async fn import_data() -> Result<Option<String>, String> {
    let p = tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file()).await.map_err(|e| e.to_string())?;
    match p { Some(p) => fs::read_to_string(p).map(Some).map_err(|e| e.to_string()), None => Ok(None) }
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
async fn fetch_cover(name: String, key: String, dim: String, kind: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let auth = format!("Authorization: Bearer {}", key);
        let u1 = format!("https://www.steamgriddb.com/api/v2/search/autocomplete/{}", enc(&name));
        let j: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u1.as_str()])?).map_err(|_| "رد غير صالح (تأكد من المفتاح)".to_string())?;
        let id = j["data"][0]["id"].as_i64().ok_or("لا نتائج لهذا الاسم")?;
        let kind = if kind == "heroes" { "heroes" } else { "grids" };
        let q = if kind == "grids" { format!("?dimensions={}", dim) } else { String::new() };
        let u2 = format!("https://www.steamgriddb.com/api/v2/{}/game/{}{}", kind, id, q);
        let g: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u2.as_str()])?).map_err(|_| "رد غير صالح".to_string())?;
        let url = g["data"][0]["url"].as_str().ok_or("لا يوجد بوستر")?.to_string();
        let file = base_dir().join("covers").join(format!("{}_{}.img", id, kind));
        let fs_ = file.to_string_lossy().to_string();
        curl(&["-s", "-L", "-o", fs_.as_str(), url.as_str()])?;
        Ok(fs_)
    }).await.map_err(|e| e.to_string())?
}

fn sgdb_id(name: &str, key: &str) -> Result<i64, String> {
    let auth = format!("Authorization: Bearer {}", key);
    let u = format!("https://www.steamgriddb.com/api/v2/search/autocomplete/{}", enc(name));
    let j: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u.as_str()])?).map_err(|_| "رد غير صالح (تأكد من المفتاح)".to_string())?;
    j["data"][0]["id"].as_i64().ok_or_else(|| "لا نتائج لهذا الاسم".to_string())
}
fn shuffle<T>(v: &mut Vec<T>) {
    let mut s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(88172645463325252) | 1;
    for i in (1..v.len()).rev() {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        let j = (s % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}
#[tauri::command]
async fn sgdb_list(name: String, key: String, dim: String, kind: String, count: usize) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let id = sgdb_id(&name, &key)?;
        let auth = format!("Authorization: Bearer {}", key);
        let kind = if kind == "heroes" { "heroes" } else { "grids" };
        let mut tries: Vec<String> = Vec::new();
        if kind == "grids" && !dim.is_empty() { tries.push(format!("?types=static&dimensions={}", dim)); }
        tries.push("?types=static".to_string());
        for q in tries {
            let u = format!("https://www.steamgriddb.com/api/v2/{}/game/{}{}", kind, id, q);
            let g: Value = serde_json::from_slice(&curl(&["-s", "-L", "-H", auth.as_str(), u.as_str()])?).map_err(|_| "رد غير صالح".to_string())?;
            if let Some(arr) = g["data"].as_array() {
                let mut items: Vec<Value> = arr.iter().filter(|x| x["url"].is_string()).map(|x| {
                    let url = x["url"].as_str().unwrap_or("");
                    json!({ "url": url, "thumb": x["thumb"].as_str().unwrap_or(url) })
                }).collect();
                if !items.is_empty() {
                    shuffle(&mut items);
                    items.truncate(count.max(1));
                    return Ok(Value::Array(items).to_string());
                }
            }
        }
        Ok("[]".to_string())
    }).await.map_err(|e| e.to_string())?
}
fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let t = t.trim().trim_end_matches('.').split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() { "game".to_string() } else { t.chars().take(80).collect() }
}
fn kind_suffix(kind: &str) -> &'static str { if kind == "hero" { "hero" } else { "poster" } }
#[tauri::command]
async fn save_image(url: String, name: String, kind: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let dir = base_dir().join("covers");
        let base = format!("{}_{}", safe_name(&name), kind_suffix(&kind));
        let clean = url.to_lowercase();
        let clean = clean.split('?').next().unwrap_or("").to_string();
        let ext = ["png", "jpg", "jpeg", "webp"].iter().find(|e| clean.ends_with(&format!(".{}", e))).copied().unwrap_or("png");
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_stem().map(|s| s.to_string_lossy().as_ref() == base.as_str()).unwrap_or(false) { let _ = fs::remove_file(&p); }
            }
        }
        let file = dir.join(format!("{}.{}", base, ext));
        let fstr = file.to_string_lossy().to_string();
        curl(&["-s", "-L", "-o", fstr.as_str(), url.as_str()])?;
        if fs::metadata(&file).map(|m| m.len()).unwrap_or(0) == 0 { return Err("فشل تنزيل الصورة".to_string()); }
        Ok(fstr)
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
fn rename_media(path: String, name: String, kind: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    let dir = base_dir().join("covers");
    if !p.exists() || p.parent() != Some(dir.as_path()) { return Ok(path); }
    let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "png".to_string());
    let new = dir.join(format!("{}_{}.{}", safe_name(&name), kind_suffix(&kind), ext));
    if new == p { return Ok(path); }
    let _ = fs::remove_file(&new);
    fs::rename(&p, &new).map_err(|e| e.to_string())?;
    Ok(new.to_string_lossy().to_string())
}
#[tauri::command]
fn read_cover(path: String) -> Result<String, String> {
    let b = fs::read(&path).map_err(|e| e.to_string())?;
    let mime = if b.starts_with(&[0x89, 0x50]) { "image/png" } else if b.starts_with(&[0xFF, 0xD8]) { "image/jpeg" } else if b.len() > 12 && &b[8..12] == b"WEBP" { "image/webp" } else { "image/png" };
    Ok(format!("data:{};base64,{}", mime, STANDARD.encode(b)))
}

#[cfg(windows)]
fn xinput_batteries() -> Vec<Value> {
    use windows_sys::Win32::UI::Input::XboxController::*;
    let mut out = Vec::new();
    for i in 0..4u32 {
        unsafe {
            let mut st: XINPUT_STATE = std::mem::zeroed();
            if XInputGetState(i, &mut st) != 0 { continue; }
            let mut b: XINPUT_BATTERY_INFORMATION = std::mem::zeroed();
            if XInputGetBatteryInformation(i, 0, &mut b) != 0 { continue; }
            let lvl = match b.BatteryLevel { 0 => 5, 1 => 33, 2 => 66, _ => 100 };
            out.push(json!({"kind": "xbox", "level": lvl, "wired": b.BatteryType == 1, "charging": false}));
        }
    }
    out
}
fn parse_ps(ds5: bool, b: &[u8]) -> Option<Value> {
    if b.is_empty() { return None; }
    if ds5 {
        let v = match (b[0], b.len()) { (0x01, n) if n >= 54 => b[53], (0x31, n) if n >= 55 => b[54], _ => return None };
        let lvl = ((v & 0x0f) as u32 * 10 + 5).min(100);
        let st = v >> 4;
        Some(json!({"kind": "ps", "level": if st == 2 { 100 } else { lvl }, "charging": st == 1 || st == 2, "wired": false}))
    } else {
        let v = match (b[0], b.len()) { (0x01, n) if n >= 31 => b[30], (0x11, n) if n >= 33 => b[32], _ => return None };
        let cable = v & 0x10 != 0;
        let raw = (v & 0x0f) as u32;
        let lvl = if cable { (raw * 100 / 11).min(100) } else { (raw * 100 / 8).min(100) };
        Some(json!({"kind": "ps", "level": lvl, "charging": cable, "wired": false}))
    }
}
fn ps_batteries() -> Vec<Value> {
    let mut out = Vec::new();
    let api = match hidapi::HidApi::new() { Ok(a) => a, Err(_) => return out };
    for d in api.device_list() {
        if d.vendor_id() != 0x054c { continue; }
        let pid = d.product_id();
        let ds5 = pid == 0x0ce6 || pid == 0x0df2;
        let ds4 = pid == 0x05c4 || pid == 0x09cc || pid == 0x0ba0;
        if !ds5 && !ds4 { continue; }
        if let Ok(dev) = api.open_path(d.path()) {
            let mut f = [0u8; 64];
            f[0] = if ds5 { 0x05 } else { 0x02 };
            let _ = dev.get_feature_report(&mut f);
            let mut buf = [0u8; 128];
            for _ in 0..8 {
                if let Ok(n) = dev.read_timeout(&mut buf, 150) {
                    if n > 0 {
                        if let Some(v) = parse_ps(ds5, &buf[..n]) { out.push(v); break; }
                    }
                }
            }
        }
    }
    out
}
#[tauri::command]
async fn gamepad_battery() -> String {
    tauri::async_runtime::spawn_blocking(|| {
        let mut v: Vec<Value> = Vec::new();
        #[cfg(windows)]
        v.extend(xinput_batteries());
        v.extend(ps_batteries());
        Value::Array(v).to_string()
    }).await.unwrap_or_else(|_| "[]".to_string())
}
#[tauri::command]
fn quit(app: tauri::AppHandle) { app.exit(0); }

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![load_data, save_data, list_drives, pick_folder, pick_file, export_data, import_data, gamepad_battery, scan, launch, open_path, fetch_cover, sgdb_list, save_image, rename_media, read_cover, quit])
        .run(tauri::generate_context!())
        .expect("error while running app");
}
