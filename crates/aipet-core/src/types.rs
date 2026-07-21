use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCreationRequest {
    pub pet_name: String,
    pub description: String,
    pub reference_image: Option<String>,
    pub style_preset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetCreationProgress {
    pub task_id: String,
    pub step: u32,
    pub sub_step: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetManifest {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub spritesheet_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlasRow {
    pub state: String,
    pub display_name: String,
    pub frames: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PetAtlas {
    pub cell_width: u32,
    pub cell_height: u32,
    pub rows: Vec<PetAtlasRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreationProgressFile {
    pub task_id: String,
    pub request: PetCreationRequest,
    pub base_image_done: bool,
    pub completed_strips: Vec<String>,
    pub total_rows: usize,
}

pub const CELL_WIDTH: u32 = 192;
pub const CELL_HEIGHT: u32 = 208;
pub const CHROMA_KEY: &str = "#FF00FF";
pub const ATLAS_COLS: u32 = 8;
pub const ATLAS_ROWS: u32 = 9;

pub fn default_atlas() -> PetAtlas {
    PetAtlas {
        cell_width: CELL_WIDTH,
        cell_height: CELL_HEIGHT,
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

pub fn sanitize_pet_id(name: &str) -> String {
    let id: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        format!("pet-{}", &uuid::Uuid::new_v4().to_string()[..8])
    } else {
        id
    }
}
