use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const REMOTE_CONFIG_FILE: &str = "remote-server.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CreationMode {
    Local,
    Remote,
}

impl Default for CreationMode {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerConfig {
    #[serde(default)]
    pub mode: CreationMode,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    "http://127.0.0.1:8787".into()
}

impl Default for RemoteServerConfig {
    fn default() -> Self {
        Self {
            mode: CreationMode::Local,
            base_url: default_base_url(),
        }
    }
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(root.join(REMOTE_CONFIG_FILE))
}

pub fn read_remote_config(app: &AppHandle) -> Result<RemoteServerConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(RemoteServerConfig::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read remote config: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse remote config: {e}"))
}

fn write_remote_config(app: &AppHandle, config: &RemoteServerConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    let data = serde_json::to_string_pretty(config).map_err(|e| format!("serialize: {e}"))?;
    fs::write(path, data).map_err(|e| format!("write remote config: {e}"))
}

#[tauri::command]
pub fn get_remote_server_config(app: AppHandle) -> Result<RemoteServerConfig, String> {
    read_remote_config(&app)
}

#[tauri::command]
pub fn save_remote_server_config(app: AppHandle, config: RemoteServerConfig) -> Result<(), String> {
    let mut cfg = config;
    cfg.base_url = cfg.base_url.trim().trim_end_matches('/').to_string();
    if cfg.base_url.is_empty() {
        return Err("服务地址不能为空".into());
    }
    if !(cfg.base_url.starts_with("http://") || cfg.base_url.starts_with("https://")) {
        return Err("服务地址必须以 http:// 或 https:// 开头".into());
    }
    write_remote_config(&app, &cfg)
}
