use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

const CONFIG_FILE: &str = "config.json";
const STATE_CONFIG_FILE: &str = "state-config.json";
const PETS_DIR: &str = "pets";
const PET_MANIFEST: &str = "pet.json";
const PET_ATLAS: &str = "pet-atlas.json";

/// Global application preferences persisted under the app data directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub always_on_top: bool,
    pub auto_start: bool,
    pub animation_speed: f64,
    pub animation_scale: f64,
    pub active_pet_id: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            always_on_top: true,
            auto_start: false,
            animation_speed: 0.6,
            animation_scale: 1.0,
            active_pet_id: String::new(),
        }
    }
}

/// One animation state row inside `pet-atlas.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlasRow {
    pub state: String,
    pub display_name: String,
    pub frames: u32,
}

/// Extended atlas layout, kept beside legacy `pet.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlas {
    pub cell_width: u32,
    pub cell_height: u32,
    pub rows: Vec<PetAtlasRow>,
}

/// Legacy Petdex manifest shape (do not extend this file).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TriggerConfig {
    ProcessFocus {
        processes: Vec<String>,
    },
    HighResource {
        resource: String,
        threshold: u32,
    },
    AudioPlaying,
    MicrophoneActive,
    ContinuousFocus {
        minutes: u32,
    },
    ComputerIdle {
        minutes: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMapping {
    pub state: String,
    pub trigger: TriggerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateConfig {
    pub mappings: Vec<StateMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStateMapping {
    state: String,
    #[serde(default)]
    processes: Vec<String>,
    trigger: Option<TriggerConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyStateConfig {
    mappings: Vec<LegacyStateMapping>,
}

impl Default for StateConfig {
    fn default() -> Self {
        StateConfig {
            mappings: vec![
                StateMapping {
                    state: "jumping".into(),
                    trigger: TriggerConfig::MicrophoneActive,
                },
                StateMapping {
                    state: "waiting".into(),
                    trigger: TriggerConfig::ComputerIdle { minutes: 5 },
                },
                StateMapping {
                    state: "idle".into(),
                    trigger: TriggerConfig::ContinuousFocus { minutes: 60 },
                },
                StateMapping {
                    state: "failed".into(),
                    trigger: TriggerConfig::HighResource {
                        resource: "cpu".into(),
                        threshold: 90,
                    },
                },
                StateMapping {
                    state: "waving".into(),
                    trigger: TriggerConfig::AudioPlaying,
                },
                StateMapping {
                    state: "running".into(),
                    trigger: TriggerConfig::ProcessFocus {
                        processes: vec![
                            "cursor.exe".into(),
                            "code.exe".into(),
                            "rider.exe".into(),
                            "unity.exe".into(),
                            "devenv.exe".into(),
                        ],
                    },
                },
                StateMapping {
                    state: "review".into(),
                    trigger: TriggerConfig::ProcessFocus {
                        processes: vec![
                            "photoshop.exe".into(),
                            "ps.exe".into(),
                            "afterfx.exe".into(),
                            "ae.exe".into(),
                        ],
                    },
                },
            ],
        }
    }
}

fn migrate_legacy_config(legacy: LegacyStateConfig) -> StateConfig {
    let mappings: Vec<StateMapping> = legacy
        .mappings
        .into_iter()
        .filter_map(|m| {
            if let Some(trigger) = m.trigger {
                return Some(StateMapping {
                    state: m.state,
                    trigger,
                });
            }
            if m.processes.is_empty() {
                return None;
            }
            Some(StateMapping {
                state: m.state,
                trigger: TriggerConfig::ProcessFocus {
                    processes: m.processes,
                },
            })
        })
        .collect();
    if mappings.is_empty() {
        return StateConfig::default();
    }
    StateConfig { mappings }
}

fn load_state_config_from_disk(path: &Path) -> Result<StateConfig, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if let Ok(cfg) = serde_json::from_str::<StateConfig>(&data) {
        return Ok(cfg);
    }
    let legacy: LegacyStateConfig =
        serde_json::from_str(&data).map_err(|e| format!("parse state config: {e}"))?;
    let migrated = migrate_legacy_config(legacy);
    write_json(path, &migrated)?;
    Ok(migrated)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetListItem {
    pub folder_id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetDetailDto {
    pub folder_id: String,
    pub manifest: PetManifest,
    pub atlas: PetAtlas,
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))
}

pub fn ensure_app_layout(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app_data_dir(app)?;
    let pets = root.join(PETS_DIR);
    fs::create_dir_all(&pets).map_err(|e| format!("create pets dir: {e}"))?;
    Ok(root)
}

/// Copy bundled default pets into the user's pets directory on first launch.
/// Only runs when the pets directory is empty (no subdirectories with pet.json).
pub fn seed_default_pets(app: &AppHandle) -> Result<(), String> {
    let root = ensure_app_layout(app)?;
    let pets_dir = root.join(PETS_DIR);

    let has_pets = fs::read_dir(&pets_dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.path().is_dir() && e.path().join(PET_MANIFEST).exists())
        })
        .unwrap_or(false);
    if has_pets {
        return Ok(());
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("resolve resource dir: {e}"))?
        .join("bundled-pets");
    if !resource_dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(&resource_dir).map_err(|e| format!("read bundled-pets: {e}"))?;
    let mut first_pet_id: Option<String> = None;

    for entry in entries.flatten() {
        let src = entry.path();
        if !src.is_dir() {
            continue;
        }
        if !src.join(PET_MANIFEST).exists() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().to_string();
        let dest = pets_dir.join(&folder_name);
        copy_dir_recursive(&src, &dest)?;
        if first_pet_id.is_none() {
            first_pet_id = Some(folder_name);
        }
    }

    if let Some(pet_id) = first_pet_id {
        let config_path = root.join(CONFIG_FILE);
        let mut cfg = if config_path.exists() {
            read_json::<AppConfig>(&config_path).unwrap_or_default()
        } else {
            AppConfig::default()
        };
        if cfg.active_pet_id.is_empty() {
            cfg.active_pet_id = pet_id;
            write_json(&config_path, &cfg)?;
        }
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("create dir {}: {e}", dest.display()))?;
    let entries = fs::read_dir(src).map_err(|e| format!("read dir {}: {e}", src.display()))?;
    for entry in entries.flatten() {
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path).map_err(|e| {
                format!("copy {} -> {}: {e}", src_path.display(), dest_path.display())
            })?;
        }
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir {}: {e}", parent.display()))?;
    }
    let data =
        serde_json::to_string_pretty(value).map_err(|e| format!("serialize json: {e}"))?;
    fs::write(path, data).map_err(|e| format!("write {}: {e}", path.display()))
}

fn default_atlas() -> PetAtlas {
    PetAtlas {
        cell_width: 192,
        cell_height: 208,
        rows: vec![
            atlas_row("idle", "待机", 6),
            atlas_row("running-right", "向右跑", 8),
            atlas_row("running-left", "向左跑", 8),
            atlas_row("waving", "挥手", 4),
            atlas_row("jumping", "跳跃", 5),
            atlas_row("failed", "失败", 8),
            atlas_row("waiting", "等待", 6),
            atlas_row("running", "专注", 6),
            atlas_row("review", "审视", 6),
        ],
    }
}

fn atlas_row(state: &str, display_name: &str, frames: u32) -> PetAtlasRow {
    PetAtlasRow {
        state: state.to_string(),
        display_name: display_name.to_string(),
        frames,
    }
}

fn load_or_create_atlas(pet_dir: &Path) -> Result<PetAtlas, String> {
    let atlas_path = pet_dir.join(PET_ATLAS);
    if atlas_path.exists() {
        return read_json(&atlas_path);
    }
    let atlas = default_atlas();
    write_json(&atlas_path, &atlas)?;
    Ok(atlas)
}

fn pet_dir(app: &AppHandle, pet_folder_id: &str) -> Result<PathBuf, String> {
    let root = ensure_app_layout(app)?;
    Ok(root.join(PETS_DIR).join(pet_folder_id))
}

fn sync_autostart_internal(app: &AppHandle, enabled: bool) {
    let autolaunch = app.autolaunch();
    if enabled {
        if let Err(e) = autolaunch.enable() {
            eprintln!("autostart enable: {e}");
        }
        return;
    }
    let Ok(is_on) = autolaunch.is_enabled() else {
        return;
    };
    if !is_on {
        return;
    }
    if let Err(e) = autolaunch.disable() {
        eprintln!("autostart disable: {e}");
    }
}

fn apply_window_runtime(app: &AppHandle, cfg: &AppConfig) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_always_on_top(cfg.always_on_top);
        let w = (192.0_f64 * cfg.animation_scale).round() as u32;
        let h = (208.0_f64 * cfg.animation_scale).round() as u32;
        let _ = main.set_size(tauri::LogicalSize::new(w, h));
    }
}

pub fn apply_runtime_from_config(app: &AppHandle, cfg: &AppConfig) {
    apply_window_runtime(app, cfg);
    sync_autostart_internal(app, cfg.auto_start);
}

pub fn apply_runtime_from_config_delta(app: &AppHandle, cfg: &AppConfig, prev: &AppConfig) {
    apply_window_runtime(app, cfg);
    if prev.auto_start != cfg.auto_start {
        sync_autostart_internal(app, cfg.auto_start);
    }
}

pub fn read_or_create_app_config(app: &AppHandle) -> Result<AppConfig, String> {
    let root = ensure_app_layout(app)?;
    let path = root.join(CONFIG_FILE);
    if !path.exists() {
        let cfg = AppConfig::default();
        write_json(&path, &cfg)?;
        return Ok(cfg);
    }
    read_json(&path)
}

#[tauri::command]
pub fn get_app_config(app: AppHandle) -> Result<AppConfig, String> {
    read_or_create_app_config(&app)
}

#[tauri::command]
pub fn save_app_config(app: AppHandle, mut config: AppConfig) -> Result<(), String> {
    let prev = read_or_create_app_config(&app)?;
    if config.animation_speed < 0.1 {
        config.animation_speed = 0.1;
    }
    if config.animation_speed > 3.0 {
        config.animation_speed = 3.0;
    }
    if config.animation_scale < 0.5 {
        config.animation_scale = 0.5;
    }
    if config.animation_scale > 3.0 {
        config.animation_scale = 3.0;
    }
    let root = ensure_app_layout(&app)?;
    write_json(&root.join(CONFIG_FILE), &config)?;
    apply_runtime_from_config_delta(&app, &config, &prev);
    let _ = app.emit("app-config-changed", &config);
    Ok(())
}

#[tauri::command]
pub fn get_state_config(app: AppHandle) -> Result<StateConfig, String> {
    let root = ensure_app_layout(&app)?;
    let path = root.join(STATE_CONFIG_FILE);
    if !path.exists() {
        let cfg = StateConfig::default();
        write_json(&path, &cfg)?;
        return Ok(cfg);
    }
    load_state_config_from_disk(&path)
}

#[tauri::command]
pub fn get_app_data_path(app: AppHandle) -> Result<String, String> {
    let root = ensure_app_layout(&app)?;
    Ok(root.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_app_data_dir(app: AppHandle) -> Result<(), String> {
    let root = ensure_app_layout(&app)?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(root)
            .spawn()
            .map_err(|e| format!("open dir: {e}"))?;
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        return Err("open_app_data_dir is only supported on Windows".into());
    }
    Ok(())
}

#[tauri::command]
pub fn save_state_config(app: AppHandle, config: StateConfig) -> Result<(), String> {
    let root = ensure_app_layout(&app)?;
    write_json(&root.join(STATE_CONFIG_FILE), &config)?;
    let _ = app.emit("state-config-changed", &config);
    Ok(())
}

#[tauri::command]
pub fn list_pets(app: AppHandle) -> Result<Vec<PetListItem>, String> {
    let root = ensure_app_layout(&app)?;
    let pets_root = root.join(PETS_DIR);
    let mut items = Vec::new();
    let entries =
        fs::read_dir(&pets_root).map_err(|e| format!("read pets dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join(PET_MANIFEST);
        if !manifest_path.exists() {
            continue;
        }
        let folder_id = entry.file_name().to_string_lossy().to_string();
        let manifest: PetManifest = read_json(&manifest_path)?;
        let spritesheet_abs = path.join(&manifest.spritesheet_path);
        let _ = load_or_create_atlas(&path)?;
        items.push(PetListItem {
            folder_id,
            display_name: manifest.display_name,
            description: manifest.description,
            spritesheet_path: spritesheet_abs.to_string_lossy().to_string(),
        });
    }
    items.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(items)
}

/// Re-scan pets directory; reserved for future side-effects (e.g. file watchers).
#[tauri::command]
pub fn refresh_pets_dir(app: AppHandle) -> Result<(), String> {
    ensure_app_layout(&app)?;
    Ok(())
}

#[tauri::command]
pub fn get_pet_detail(app: AppHandle, folder_id: String) -> Result<PetDetailDto, String> {
    let dir = pet_dir(&app, &folder_id)?;
    if !dir.exists() {
        return Err(format!("pet not found: {folder_id}"));
    }
    let manifest_path = dir.join(PET_MANIFEST);
    if !manifest_path.exists() {
        return Err(format!("missing {PET_MANIFEST} in {folder_id}"));
    }
    let manifest: PetManifest = read_json(&manifest_path)?;
    let atlas = load_or_create_atlas(&dir)?;
    Ok(PetDetailDto {
        folder_id,
        manifest,
        atlas,
    })
}

#[tauri::command]
pub fn get_pet_spritesheet_path(app: AppHandle, folder_id: String) -> Result<String, String> {
    let dir = pet_dir(&app, &folder_id)?;
    if !dir.exists() {
        return Err(format!("pet not found: {folder_id}"));
    }
    let manifest_path = dir.join(PET_MANIFEST);
    if !manifest_path.exists() {
        return Err(format!("missing {PET_MANIFEST} in {folder_id}"));
    }
    let manifest: PetManifest = read_json(&manifest_path)?;
    let spritesheet_abs = dir.join(&manifest.spritesheet_path);
    Ok(spritesheet_abs.to_string_lossy().to_string())
}

#[tauri::command]
pub fn delete_pet(app: AppHandle, folder_id: String) -> Result<(), String> {
    let dir = pet_dir(&app, &folder_id)?;
    if !dir.exists() {
        return Err(format!("pet not found: {folder_id}"));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("删除宠物文件夹失败: {e}"))?;
    Ok(())
}