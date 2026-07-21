use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::types::{
    default_atlas, PetAtlas, PetManifest, ATLAS_COLS, ATLAS_ROWS, CELL_HEIGHT, CELL_WIDTH,
};

const MAX_ZIP_BYTES: u64 = 80 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 120 * 1024 * 1024;
const ALLOWED_FILES: &[&str] = &[
    "pet.json",
    "pet-atlas.json",
    "spritesheet.webp",
    "spritesheet.png",
];

#[derive(Debug, Clone)]
pub struct PetPackageFiles {
    pub pet_json: Vec<u8>,
    pub pet_atlas_json: Vec<u8>,
    pub spritesheet_webp: Vec<u8>,
    pub spritesheet_png: Option<Vec<u8>>,
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

pub fn write_pet_zip(path: &Path, files: &PetPackageFiles) -> Result<String, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 zip 目录失败: {e}"))?;
    }
    let file = fs::File::create(path).map_err(|e| format!("创建 zip 失败: {e}"))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("pet.json", opts)
        .map_err(|e| format!("zip pet.json: {e}"))?;
    zip.write_all(&files.pet_json)
        .map_err(|e| format!("write pet.json: {e}"))?;

    zip.start_file("pet-atlas.json", opts)
        .map_err(|e| format!("zip pet-atlas.json: {e}"))?;
    zip.write_all(&files.pet_atlas_json)
        .map_err(|e| format!("write pet-atlas.json: {e}"))?;

    zip.start_file("spritesheet.webp", opts)
        .map_err(|e| format!("zip spritesheet.webp: {e}"))?;
    zip.write_all(&files.spritesheet_webp)
        .map_err(|e| format!("write spritesheet.webp: {e}"))?;

    if let Some(png) = &files.spritesheet_png {
        zip.start_file("spritesheet.png", opts)
            .map_err(|e| format!("zip spritesheet.png: {e}"))?;
        zip.write_all(png)
            .map_err(|e| format!("write spritesheet.png: {e}"))?;
    }

    zip.finish().map_err(|e| format!("finish zip: {e}"))?;
    let bytes = fs::read(path).map_err(|e| format!("读取 zip 失败: {e}"))?;
    Ok(sha256_hex(&bytes))
}

fn is_safe_entry_name(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') || name.contains("..") {
        return false;
    }
    if Path::new(name).is_absolute() {
        return false;
    }
    let parts: Vec<&str> = name.split('/').collect();
    if parts.len() != 1 {
        return false;
    }
    ALLOWED_FILES.contains(&parts[0])
}

/// Validate and extract a pet zip into `staging_dir`. Returns sanitized pet id.
pub fn extract_and_validate_pet_zip(
    zip_bytes: &[u8],
    expected_sha256: Option<&str>,
    staging_dir: &Path,
) -> Result<String, String> {
    if zip_bytes.len() as u64 > MAX_ZIP_BYTES {
        return Err(format!(
            "ZIP 过大（{} bytes），上限 {} bytes",
            zip_bytes.len(),
            MAX_ZIP_BYTES
        ));
    }
    if let Some(expected) = expected_sha256 {
        let actual = sha256_hex(zip_bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "ZIP 校验失败: SHA-256 不匹配 (期望 {}, 实际 {})",
                expected, actual
            ));
        }
    }

    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| format!("打开 ZIP 失败: {e}"))?;

    let mut uncompressed: u64 = 0;
    let mut found_pet_json = false;
    let mut found_atlas = false;
    let mut found_webp = false;

    if staging_dir.exists() {
        fs::remove_dir_all(staging_dir).map_err(|e| format!("清理暂存目录失败: {e}"))?;
    }
    fs::create_dir_all(staging_dir).map_err(|e| format!("创建暂存目录失败: {e}"))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("读取 ZIP 条目失败: {e}"))?;
        let name = file.name().to_string();
        if file.is_dir() {
            continue;
        }
        if !is_safe_entry_name(&name) {
            return Err(format!("ZIP 含有非法路径或非白名单文件: {name}"));
        }
        let size = file.size();
        uncompressed = uncompressed.saturating_add(size);
        if uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err("ZIP 解压后体积过大".into());
        }

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| format!("读取 ZIP 文件 {name} 失败: {e}"))?;
        let out = staging_dir.join(&name);
        fs::write(&out, &buf).map_err(|e| format!("写入暂存文件失败: {e}"))?;

        match name.as_str() {
            "pet.json" => found_pet_json = true,
            "pet-atlas.json" => found_atlas = true,
            "spritesheet.webp" => found_webp = true,
            _ => {}
        }
    }

    if !found_pet_json || !found_atlas || !found_webp {
        return Err("ZIP 缺少必需文件: pet.json / pet-atlas.json / spritesheet.webp".into());
    }

    let manifest: PetManifest = serde_json::from_slice(
        &fs::read(staging_dir.join("pet.json")).map_err(|e| format!("读取 pet.json: {e}"))?,
    )
    .map_err(|e| format!("解析 pet.json 失败: {e}"))?;

    if manifest.spritesheet_path != "spritesheet.webp" {
        return Err("pet.json 的 spritesheetPath 必须为 spritesheet.webp".into());
    }

    let atlas: PetAtlas = serde_json::from_slice(
        &fs::read(staging_dir.join("pet-atlas.json"))
            .map_err(|e| format!("读取 pet-atlas.json: {e}"))?,
    )
    .map_err(|e| format!("解析 pet-atlas.json 失败: {e}"))?;

    if atlas.cell_width != CELL_WIDTH || atlas.cell_height != CELL_HEIGHT {
        return Err(format!(
            "图集单元尺寸无效: {}x{}，期望 {}x{}",
            atlas.cell_width, atlas.cell_height, CELL_WIDTH, CELL_HEIGHT
        ));
    }
    if atlas.rows.is_empty() || atlas.rows.len() as u32 > ATLAS_ROWS {
        return Err("pet-atlas.json 行数无效".into());
    }
    for row in &atlas.rows {
        if row.frames == 0 || row.frames > ATLAS_COLS {
            return Err(format!("状态 {} 帧数无效: {}", row.state, row.frames));
        }
    }

    let webp_bytes = fs::read(staging_dir.join("spritesheet.webp"))
        .map_err(|e| format!("读取 spritesheet.webp: {e}"))?;
    let img = image::load_from_memory(&webp_bytes)
        .map_err(|e| format!("解码 spritesheet.webp 失败: {e}"))?;
    let expected_w = CELL_WIDTH * ATLAS_COLS;
    let expected_h = CELL_HEIGHT * ATLAS_ROWS;
    if img.width() != expected_w || img.height() != expected_h {
        return Err(format!(
            "图集尺寸不匹配: {}x{}，期望 {}x{}",
            img.width(),
            img.height(),
            expected_w,
            expected_h
        ));
    }

    let _ = default_atlas();
    let pet_id = crate::types::sanitize_pet_id(&manifest.id);
    if pet_id.is_empty() {
        return Err("宠物 id 无效".into());
    }
    Ok(pet_id)
}

/// Atomically install a validated staging pet directory into pets_dir.
/// Fails if destination already exists.
pub fn install_pet_dir(staging_dir: &Path, pets_dir: &Path, pet_id: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(pets_dir).map_err(|e| format!("create pets dir: {e}"))?;
    let dest = pets_dir.join(pet_id);
    if dest.exists() {
        return Err(format!("pet already exists: {pet_id}"));
    }
    if fs::rename(staging_dir, &dest).is_err() {
        copy_dir_recursive(staging_dir, &dest)?;
        let _ = fs::remove_dir_all(staging_dir);
    }
    Ok(dest)
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("read entry: {e}"))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("copy file: {e}"))?;
        }
    }
    Ok(())
}

/// Import zip bytes into pets_dir. Returns installed pet id.
pub fn import_pet_zip_bytes(
    zip_bytes: &[u8],
    expected_sha256: Option<&str>,
    pets_dir: &Path,
    staging_parent: &Path,
) -> Result<String, String> {
    let staging = staging_parent.join(format!("pet-import-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let pet_id = extract_and_validate_pet_zip(zip_bytes, expected_sha256, &staging)?;
        install_pet_dir(&staging, pets_dir, &pet_id)?;
        Ok(pet_id)
    })();
    if result.is_err() && staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}
