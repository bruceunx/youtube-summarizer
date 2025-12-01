use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub api_key: Option<String>,
    pub ai_url: Option<String>,
    pub ai_model_name: Option<String>,
    pub whisper_api_key: Option<String>,
    pub whisper_url: Option<String>,
    pub whisper_model_name: Option<String>,
    pub proxy: Option<String>,
    pub tube_api_url: Option<String>,
}

pub fn get_config_path(app: &tauri::AppHandle) -> PathBuf {
    let mut path = app
        .path()
        .app_config_dir()
        .expect("Faile to find the config fold");
    path.push("newscenter");
    fs::create_dir_all(&path).expect("Faile to create the config folder");
    path.push("settings.json");
    path
}

pub fn get_proxy(app: &tauri::AppHandle) -> Option<String> {
    let path = get_config_path(app);
    if !path.exists() {
        None
    } else {
        let contents = fs::read_to_string(path).ok()?;
        let setting: AppSettings = serde_json::from_str(&contents).ok()?;
        setting.proxy
    }
}

pub fn get_tube_api_url(app: &tauri::AppHandle) -> Option<String> {
    let path = get_config_path(app);
    if !path.exists() {
        None
    } else {
        let contents = fs::read_to_string(path).ok()?;
        let setting: AppSettings = serde_json::from_str(&contents).ok()?;
        setting.tube_api_url
    }
}

pub fn get_settings(app: &tauri::AppHandle) -> Option<AppSettings> {
    let path = get_config_path(app);
    if !path.exists() {
        None
    } else {
        let contents = fs::read_to_string(path).ok()?;
        let setting: AppSettings = serde_json::from_str(&contents).ok()?;
        Some(setting)
    }
}

#[tauri::command]
pub fn load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let path = get_config_path(&app);
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;

    let setting: AppSettings =
        serde_json::from_str(&contents).map_err(|e| format!("error in serizer: {}", e))?;
    Ok(setting)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = get_config_path(&app);

    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}
