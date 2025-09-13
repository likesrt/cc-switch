use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::get_app_config_dir;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TargetEnv {
    Windows,
    Wsl,
}

impl Default for TargetEnv {
    fn default() -> Self {
        TargetEnv::Windows
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_show_in_tray")]
    #[serde(rename = "showInTray")]
    pub show_in_tray: bool,

    #[serde(default)]
    #[serde(rename = "targetEnv")]
    pub target_env: TargetEnv,

    #[serde(default)]
    #[serde(rename = "wslDistro")]
    pub wsl_distro: Option<String>,
}

fn default_show_in_tray() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_in_tray: true,
            target_env: TargetEnv::Windows,
            wsl_distro: None,
        }
    }
}

pub fn get_settings_path() -> PathBuf {
    get_app_config_dir().join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = get_settings_path();
    if !path.exists() {
        return AppSettings::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str::<AppSettings>(&s).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(s: &AppSettings) -> Result<(), String> {
    let path = get_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    let txt = serde_json::to_string_pretty(s).map_err(|e| format!("序列化失败: {}", e))?;
    std::fs::write(&path, txt).map_err(|e| format!("写入设置失败: {}", e))
}

