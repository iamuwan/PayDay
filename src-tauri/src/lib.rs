//! PayDay — a personal money tracker for macOS and Windows.
//!
//! The interface lives in `src/index.html`. This file is the small native
//! side: it stores all data as a JSON file in the app's data folder
//! (`~/Library/Application Support/app.payday.tracker/` on macOS,
//! `%APPDATA%\app.payday.tracker\` on Windows), keeps the
//! previous version and a daily copy for 30 days, and writes backups to
//! the Downloads folder on request.

use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const DATA_FILE: &str = "payday.json";
const PREV_FILE: &str = "payday.prev.json";
const DAILY_KEEP: usize = 30;

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn is_valid_json(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Delete the oldest daily backups so at most `keep` remain.
fn prune(dir: &Path, keep: usize) {
    if let Ok(entries) = fs::read_dir(dir) {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map_or(false, |x| x == "json"))
            .collect();
        files.sort();
        while files.len() > keep {
            let _ = fs::remove_file(files.remove(0));
        }
    }
}

/// Read the saved data. Returns an empty string on first launch. If the main
/// file is damaged, falls back to the previous version.
#[tauri::command]
fn load_data(app: tauri::AppHandle) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let main = dir.join(DATA_FILE);
    if !main.exists() {
        return Ok(String::new());
    }
    let text = fs::read_to_string(&main).map_err(|e| e.to_string())?;
    if is_valid_json(&text) {
        return Ok(text);
    }
    let prev = fs::read_to_string(dir.join(PREV_FILE)).unwrap_or_default();
    if is_valid_json(&prev) {
        Ok(prev)
    } else {
        Err("The data file is damaged and no earlier copy could be read.".into())
    }
}

/// Save all data. Writes to a temporary file first and then swaps it in, so a
/// crash mid-write never leaves a half-written file.
#[tauri::command]
fn save_data(app: tauri::AppHandle, json: String) -> Result<(), String> {
    if !is_valid_json(&json) {
        return Err("Refusing to save data that isn't valid JSON.".into());
    }
    let dir = data_dir(&app)?;
    let main = dir.join(DATA_FILE);

    if main.exists() {
        let _ = fs::copy(&main, dir.join(PREV_FILE));
        let daily_dir = dir.join("backups");
        fs::create_dir_all(&daily_dir).map_err(|e| e.to_string())?;
        let daily = daily_dir.join(format!("payday-{}.json", today()));
        if !daily.exists() {
            let _ = fs::copy(&main, &daily);
            prune(&daily_dir, DAILY_KEEP);
        }
    }

    let tmp = dir.join(format!("{DATA_FILE}.tmp"));
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &main).map_err(|e| e.to_string())?;
    Ok(())
}

/// Write a backup copy to the Downloads folder and return its path.
#[tauri::command]
fn export_data(app: tauri::AppHandle, json: String) -> Result<String, String> {
    if !is_valid_json(&json) {
        return Err("Nothing valid to export.".into());
    }
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    let mut path = dir.join(format!("PayDay-backup-{}.json", today()));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("PayDay-backup-{}-{n}.json", today()));
        n += 1;
    }
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Where the data lives, shown in the profile screen.
#[tauri::command]
fn data_location(app: tauri::AppHandle) -> Result<String, String> {
    Ok(data_dir(&app)?.display().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_data,
            save_data,
            export_data,
            data_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running PayDay");
}
