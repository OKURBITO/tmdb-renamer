use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub api_key: String,
    pub proxy: String,
    pub media_dir: String,
    pub extras_dir: String,
    pub archive_extras: bool,
    pub append_ep_title: bool,
    pub append_season_name: bool,
    pub scan_mode: String,  // "batch" | "single"
    pub media_type: String, // "auto" | "tv" | "movie"
    pub dark_mode: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            proxy: String::new(),
            media_dir: String::new(),
            extras_dir: String::new(),
            archive_extras: true,
            append_ep_title: false,
            append_season_name: false,
            scan_mode: "batch".to_string(),
            media_type: "auto".to_string(),
            dark_mode: true,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("config.json")))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Self>(&text) {
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}
