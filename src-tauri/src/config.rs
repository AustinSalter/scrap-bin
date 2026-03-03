use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

use crate::fragment::{Disposition, SourceType};

const APP_DIR_NAME: &str = ".scrapbin";

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Could not determine home directory")]
    NoHomeDir,
    #[error("Source already exists: {0}")]
    DuplicateSource(String),
    #[error("Source not found: {0}")]
    SourceNotFound(String),
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

pub fn twitter_credentials_path() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("twitter_credentials.json"))
}

pub fn twitter_sync_path() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("twitter_sync.json"))
}

// --- Twitter credentials ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    pub username: String,
    pub expires_at: String,
}

pub fn load_twitter_credentials() -> Option<TwitterCredentials> {
    let path = twitter_credentials_path().ok()?;
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_twitter_credentials(creds: &TwitterCredentials) -> Result<(), ConfigError> {
    let path = twitter_credentials_path()?;
    let data = serde_json::to_string_pretty(creds)?;
    fs::write(&path, data)?;
    // Set 0o600 permissions on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn delete_twitter_credentials() -> Result<(), ConfigError> {
    let path = twitter_credentials_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
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
pub fn config_set(mut config: AppConfig) -> Result<(), ConfigError> {
    // Preserve the real API key if the frontend sends back the redacted sentinel.
    if config.readwise_api_key.as_deref() == Some("***") {
        let existing = load_config()?;
        config.readwise_api_key = existing.readwise_api_key;
    }
    save_config(&config)
}

#[tauri::command]
pub fn config_get_data_dir() -> Result<String, ConfigError> {
    Ok(app_data_dir()?.to_string_lossy().to_string())
}

// --- Source configuration ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub source_type: SourceType,
    pub display_name: String,
    pub config: serde_json::Value,
    pub default_disposition: Disposition,
    pub sync_schedule: Option<String>,
    pub enabled: bool,
    pub vault_subfolder: Option<String>,
}

pub fn sources_path() -> Result<PathBuf, ConfigError> {
    Ok(app_data_dir()?.join("sources.json"))
}

pub fn load_sources() -> Result<Vec<SourceConfig>, ConfigError> {
    let path = sources_path()?;
    if path.exists() {
        let data = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        Ok(Vec::new())
    }
}

pub fn save_sources(sources: &[SourceConfig]) -> Result<(), ConfigError> {
    let path = sources_path()?;
    let data = serde_json::to_string_pretty(sources)?;
    fs::write(path, data)?;
    Ok(())
}

#[tauri::command]
pub fn list_sources() -> Result<Vec<SourceConfig>, ConfigError> {
    load_sources()
}

#[tauri::command]
pub fn add_source(source: SourceConfig) -> Result<(), ConfigError> {
    let mut sources = load_sources()?;
    if sources.iter().any(|s| s.id == source.id) {
        return Err(ConfigError::DuplicateSource(source.id));
    }
    sources.push(source);
    save_sources(&sources)
}

#[tauri::command]
pub fn update_source(source: SourceConfig) -> Result<(), ConfigError> {
    let mut sources = load_sources()?;
    let pos = sources
        .iter()
        .position(|s| s.id == source.id)
        .ok_or_else(|| ConfigError::SourceNotFound(source.id.clone()))?;
    sources[pos] = source;
    save_sources(&sources)
}

#[tauri::command]
pub fn remove_source(id: String) -> Result<(), ConfigError> {
    let mut sources = load_sources()?;
    sources.retain(|s| s.id != id);
    save_sources(&sources)
}
