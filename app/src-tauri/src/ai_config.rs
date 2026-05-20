use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const AI_CONFIG_FILE: &str = "ai-config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiApiConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub provider_cache: HashMap<String, ProviderSettings>,
}

impl Default for AiApiConfig {
    fn default() -> Self {
        Self {
            provider: "dashscope".into(),
            api_key: String::new(),
            base_url: "https://dashscope.aliyuncs.com/api/v1".into(),
            model: "wan2.7-image".into(),
            provider_cache: HashMap::new(),
        }
    }
}

fn ai_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    Ok(root.join(AI_CONFIG_FILE))
}

pub fn read_ai_config(app: &AppHandle) -> Result<AiApiConfig, String> {
    let path = ai_config_path(app)?;
    if !path.exists() {
        return Ok(AiApiConfig::default());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("read ai config: {e}"))?;
    let mut cfg: AiApiConfig =
        serde_json::from_str(&data).map_err(|e| format!("parse ai config: {e}"))?;
    if cfg.provider_cache.is_empty() {
        cfg.provider_cache.insert(
            cfg.provider.clone(),
            ProviderSettings {
                api_key: cfg.api_key.clone(),
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
            },
        );
        return Ok(cfg);
    }

    if let Some(cached) = cfg.provider_cache.get(&cfg.provider) {
        if cfg.api_key.is_empty() {
            cfg.api_key = cached.api_key.clone();
        }
        if cfg.base_url.is_empty() {
            cfg.base_url = cached.base_url.clone();
        }
        if cfg.model.is_empty() {
            cfg.model = cached.model.clone();
        }
    }
    if !cfg.provider_cache.contains_key(&cfg.provider) {
        cfg.provider_cache.insert(
            cfg.provider.clone(),
            ProviderSettings {
                api_key: cfg.api_key.clone(),
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
            },
        );
    }
    Ok(cfg)
}

fn write_ai_config(app: &AppHandle, config: &AiApiConfig) -> Result<(), String> {
    let path = ai_config_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    let mut persisted = config.clone();
    persisted.provider_cache.insert(
        persisted.provider.clone(),
        ProviderSettings {
            api_key: persisted.api_key.clone(),
            base_url: persisted.base_url.clone(),
            model: persisted.model.clone(),
        },
    );
    let data = serde_json::to_string_pretty(&persisted).map_err(|e| format!("serialize: {e}"))?;
    fs::write(&path, data).map_err(|e| format!("write ai config: {e}"))
}

#[tauri::command]
pub fn get_ai_api_config(app: AppHandle) -> Result<AiApiConfig, String> {
    read_ai_config(&app)
}

#[tauri::command]
pub fn save_ai_api_config(app: AppHandle, config: AiApiConfig) -> Result<(), String> {
    write_ai_config(&app, &config)
}
