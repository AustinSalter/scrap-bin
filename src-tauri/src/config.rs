use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

const APP_DIR_NAME: &str = ".scrapbin";

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Could not determine home directory")]
    NoHomeDir,
}

impl Serialize for ConfigError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub chroma_port: u16,
    pub sidecar_port: u16,
    pub vault_path: Option<String>,
    pub readwise_api_key: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            chroma_port: 8000,
            sidecar_port: 50051,
            vault_path: None,
            readwise_api_key: None,
        }
    }
}

/// Returns `~/.scrapbin/`
pub fn app_data_dir() -> Result<PathBuf, ConfigError> {
    let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
    Ok(home.join(APP_DIR_NAME))
}

/// Ensures the directory structure exists:
/// ~/.scrapbin/
///   chroma/        — Chroma persistence
///   config.json    — app configuration
///   index_state.json — incremental indexing state
///   logs/          — log files
pub fn init_app_dirs() -> Result<PathBuf, ConfigError> {
    let base = app_data_dir()?;
    let dirs = [
        base.join("chroma"),
        base.join("logs"),
    ];
    for dir in &dirs {
        fs::create_dir_all(dir)?;
    }
    Ok(base)
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("config.json"))
}

pub fn index_state_path() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("index_state.json"))
}

pub fn chroma_persist_dir() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("chroma"))
}

pub fn load_config() -> Result<AppConfig, ConfigError> {
    let path = config_path()?;
    if path.exists() {
        let data = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        let config = AppConfig::default();
        save_config(&config)?;
        Ok(config)
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    let path = config_path()?;
    let data = serde_json::to_string_pretty(config)?;
    fs::write(path, data)?;
    Ok(())
}

// --- Tauri commands ---

#[tauri::command]
pub fn config_get() -> Result<AppConfig, ConfigError> {
    let mut config = load_config()?;
    // Redact the API key for frontend display — only expose whether it is set.
    if config.readwise_api_key.is_some() {
        config.readwise_api_key = Some("***".to_string());
    }
    Ok(config)
}

#[tauri::command]
pub fn config_set(config: AppConfig) -> Result<(), ConfigError> {
    save_config(&config)
}

#[tauri::command]
pub fn config_get_data_dir() -> Result<String, ConfigError> {
    Ok(app_data_dir()?.to_string_lossy().to_string())
}
