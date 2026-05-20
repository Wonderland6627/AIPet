use chrono::Local;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::ai_api_client::{AiApiClient, ImageSize};
use crate::ai_config::read_ai_config;
use crate::config_manager::{ensure_app_layout, PetAtlas, PetAtlasRow, PetManifest};
use crate::image_processor;
use crate::prompt_builder::{self, PetPromptConfig};

const CHROMA_KEY: &str = "#FF00FF";
const CELL_WIDTH: u32 = 192;
const CELL_HEIGHT: u32 = 208;
const RATE_LIMIT_BACKOFF_SECS: [u64; 3] = [5, 15, 30];

mod layout_guides {
    pub fn get(state: &str) -> Option<&'static [u8]> {
        match state {
            "idle" => Some(include_bytes!("../assets/layout-guides/idle.png")),
            "running-right" => Some(include_bytes!("../assets/layout-guides/running-right.png")),
            "running-left" => Some(include_bytes!("../assets/layout-guides/running-left.png")),
            "waving" => Some(include_bytes!("../assets/layout-guides/waving.png")),
            "jumping" => Some(include_bytes!("../assets/layout-guides/jumping.png")),
            "failed" => Some(include_bytes!("../assets/layout-guides/failed.png")),
            "waiting" => Some(include_bytes!("../assets/layout-guides/waiting.png")),
            "running" => Some(include_bytes!("../assets/layout-guides/running.png")),
            "review" => Some(include_bytes!("../assets/layout-guides/review.png")),
            _ => None,
        }
    }
}

/// Split total_frames into batches each ≤ max_per_batch.
/// Distributes evenly: e.g., 6 with max 4 → [3, 3], 8 with max 4 → [4, 4].
fn compute_batches(total_frames: u32, max_per_batch: u32) -> Vec<u32> {
    if total_frames <= max_per_batch {
        return vec![total_frames];
    }
    let num_batches = (total_frames + max_per_batch - 1) / max_per_batch;
    let base = total_frames / num_batches;
    let remainder = total_frames % num_batches;
    let mut batches = Vec::with_capacity(num_batches as usize);
    for i in 0..num_batches {
        batches.push(base + if i < remainder { 1 } else { 0 });
    }
    batches
}

/// Decide max frames per API call based on provider image ratio limits.
fn max_frames_for_provider(provider: &str) -> u32 {
    if provider == "openai" {
        return 3; // 3 * 192 / 208 = 2.77 <= 3:1
    }
    4 // DashScope uses 4:1, Gemini currently has no strict strip ratio cap
}

fn is_rate_limit_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("429") {
        return true;
    }
    if normalized.contains("too many requests") {
        return true;
    }
    normalized.contains("engineoverloaded")
}

/// Concatenate multiple horizontal strip images into one wide strip.
fn concatenate_strips(strips: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let images: Vec<image::DynamicImage> = strips
        .iter()
        .map(|b| image::load_from_memory(b).map_err(|e| format!("加载批次图片失败: {e}")))
        .collect::<Result<Vec<_>, _>>()?;

    let total_width: u32 = images.iter().map(|img| img.width()).sum();
    let height = images[0].height();
    let mut combined = RgbaImage::new(total_width, height);

    let mut x_offset = 0u32;
    for img in &images {
        let rgba = img.to_rgba8();
        for y in 0..rgba.height() {
            for x in 0..rgba.width() {
                combined.put_pixel(x_offset + x, y, *rgba.get_pixel(x, y));
            }
        }
        x_offset += img.width();
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(combined)
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("拼接条带编码失败: {e}"))?;
    Ok(buf.into_inner())
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
pub struct PetCreationResult {
    pub task_id: String,
    pub pet_id: String,
}

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
struct CreationProgressFile {
    pub task_id: String,
    pub request: PetCreationRequest,
    pub base_image_done: bool,
    pub completed_strips: Vec<String>,
    pub total_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompleteCreationTask {
    pub work_dir_name: String,
    pub pet_name: String,
    pub base_image_done: bool,
    pub completed_count: usize,
    pub total_rows: usize,
}

struct ActiveTask {
    cancel_token: CancellationToken,
}

static TASKS: std::sync::OnceLock<Arc<Mutex<HashMap<String, ActiveTask>>>> =
    std::sync::OnceLock::new();

fn tasks() -> &'static Arc<Mutex<HashMap<String, ActiveTask>>> {
    TASKS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn progress_file_path(work_dir: &Path) -> PathBuf {
    work_dir.join("progress.json")
}

fn create_initial_progress(
    task_id: &str,
    request: &PetCreationRequest,
    total_rows: usize,
) -> CreationProgressFile {
    CreationProgressFile {
        task_id: task_id.to_string(),
        request: request.clone(),
        base_image_done: false,
        completed_strips: Vec::new(),
        total_rows,
    }
}

fn load_progress_file(work_dir: &Path) -> Result<CreationProgressFile, String> {
    let path = progress_file_path(work_dir);
    let data = fs::read_to_string(&path).map_err(|e| format!("读取进度文件失败: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("解析进度文件失败: {e}"))
}

fn save_progress_file(work_dir: &Path, progress: &CreationProgressFile) -> Result<(), String> {
    let path = progress_file_path(work_dir);
    let data = serde_json::to_string_pretty(progress).map_err(|e| format!("序列化进度失败: {e}"))?;
    fs::write(path, data).map_err(|e| format!("写入进度文件失败: {e}"))
}

fn mark_strip_completed(progress: &mut CreationProgressFile, state_name: &str) {
    if progress.completed_strips.iter().any(|s| s == state_name) {
        return;
    }
    progress.completed_strips.push(state_name.to_string());
}

fn emit_progress(app: &AppHandle, progress: &PetCreationProgress) {
    let _ = app.emit("pet-creation-progress", progress);
}

fn emit_log(app: &AppHandle, task_id: &str, level: &str, msg: &str) {
    let ts = Local::now().format("%H:%M:%S%.3f").to_string();
    let _ = app.emit(
        "pet-creation-log",
        serde_json::json!({
            "taskId": task_id,
            "time": ts,
            "level": level,
            "message": msg
        }),
    );
}

fn make_progress(
    task_id: &str,
    step: u32,
    sub_step: &str,
    status: &str,
    message: &str,
) -> PetCreationProgress {
    PetCreationProgress {
        task_id: task_id.to_string(),
        step,
        sub_step: sub_step.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        error: None,
    }
}

fn sanitize_pet_id(name: &str) -> String {
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
        format!("pet-{}", &Uuid::new_v4().to_string()[..8])
    } else {
        id
    }
}

fn write_creation_log(app: &AppHandle, task_id: &str, pet_name: &str, success: bool, detail: &str) {
    let Ok(root) = ensure_app_layout(app) else {
        return;
    };
    let logs_dir = root.join("logs");
    let _ = fs::create_dir_all(&logs_dir);
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let status_str = if success { "success" } else { "failed" };
    let filename = format!("pet_creation_{}_{}_{}_{}.log", pet_name, status_str, timestamp, &task_id[..8]);
    let log_content = format!(
        "Task ID: {}\nPet Name: {}\nStatus: {}\nTime: {}\n\n{}",
        task_id,
        pet_name,
        status_str,
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        detail
    );
    let _ = fs::write(logs_dir.join(filename), log_content);
}

async fn run_creation_pipeline(
    app: AppHandle,
    task_id: String,
    request: PetCreationRequest,
    cancel_token: CancellationToken,
    work_dir_name: Option<String>,
) {
    let result = run_pipeline_inner(
        &app,
        &task_id,
        &request,
        &cancel_token,
        work_dir_name.as_deref(),
    )
    .await;
    match result {
        Ok(pet_id) => {
            write_creation_log(&app, &task_id, &request.pet_name, true, "生成成功");
            let _ = app.emit(
                "pet-creation-completed",
                PetCreationResult {
                    task_id: task_id.clone(),
                    pet_id,
                },
            );
        }
        Err(e) => {
            if cancel_token.is_cancelled() {
                write_creation_log(&app, &task_id, &request.pet_name, false, "用户取消");
                let _ = app.emit(
                    "pet-creation-cancelled",
                    serde_json::json!({ "taskId": task_id, "reason": "用户取消" }),
                );
            } else {
                write_creation_log(&app, &task_id, &request.pet_name, false, &e);
                let _ = app.emit(
                    "pet-creation-failed",
                    serde_json::json!({ "taskId": task_id, "reason": e }),
                );
            }
        }
    }

    let mut map = tasks().lock().await;
    map.remove(&task_id);
}

async fn run_pipeline_inner(
    app: &AppHandle,
    task_id: &str,
    request: &PetCreationRequest,
    cancel_token: &CancellationToken,
    resume_work_dir_name: Option<&str>,
) -> Result<String, String> {
    // Step 1: Prepare work directory
    emit_progress(app, &make_progress(task_id, 1, "", "running", "准备工作目录..."));

    let ai_config = read_ai_config(app)?;
    let ai_provider = ai_config.provider.clone();
    let client = AiApiClient::new(ai_config)?;

    let root = ensure_app_layout(app)?;
    let pet_id = sanitize_pet_id(&request.pet_name);
    let work_dir_name = if let Some(name) = resume_work_dir_name {
        name.to_string()
    } else {
        format!("pet-creation-{}", task_id)
    };
    let work_dir = root.join("tmp").join(work_dir_name);
    fs::create_dir_all(&work_dir).map_err(|e| format!("创建工作目录失败: {e}"))?;
    let row_count = prompt_builder::row_count();
    let mut progress = if progress_file_path(&work_dir).exists() {
        load_progress_file(&work_dir)?
    } else {
        create_initial_progress(task_id, request, row_count)
    };
    progress.task_id = task_id.to_string();
    progress.request = request.clone();
    progress.total_rows = row_count;
    save_progress_file(&work_dir, &progress)?;

    let prompt_config = PetPromptConfig {
        pet_name: request.pet_name.clone(),
        description: request.description.clone(),
        style_preset: request.style_preset.clone(),
        chroma_key: CHROMA_KEY.into(),
    };

    emit_progress(app, &make_progress(task_id, 1, "", "completed", "工作目录已准备"));

    check_cancelled(cancel_token)?;

    // Step 2: Generate base image
    emit_progress(app, &make_progress(task_id, 2, "", "running", "生成基础形象..."));

    let base_prompt = prompt_builder::build_base_prompt(&prompt_config);
    let base_size = ImageSize {
        width: CELL_WIDTH,
        height: CELL_HEIGHT,
    };
    let base_path = work_dir.join("base.png");
    let has_cached_base = base_path.exists();

    emit_log(app, task_id, "info", &format!("基础形象 prompt: {}", &base_prompt[..base_prompt.len().min(200)]));
    emit_log(app, task_id, "info", &format!("请求尺寸: {}x{}", base_size.width, base_size.height));

    let base_image_bytes = if has_cached_base {
        emit_log(app, task_id, "info", "检测到可复用基础形象，跳过生成");
        let cached = fs::read(&base_path).map_err(|e| format!("读取缓存 base 图片失败: {e}"))?;
        if !progress.base_image_done {
            progress.base_image_done = true;
            save_progress_file(&work_dir, &progress)?;
        }
        cached
    } else if let Some(ref_img_b64) = &request.reference_image {
        let generated = if ref_img_b64.is_empty() {
            emit_log(app, task_id, "info", "调用 text_to_image API...");
            client.text_to_image(&base_prompt, base_size).await?
        } else {
            let ref_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                ref_img_b64,
            )
            .map_err(|e| format!("解码参考图失败: {e}"))?;
            emit_log(app, task_id, "info", &format!("调用 text_and_image_to_image API (参考图 {} bytes)...", ref_bytes.len()));
            client
                .text_and_image_to_image(&base_prompt, &ref_bytes, base_size)
                .await?
        };
        emit_log(app, task_id, "info", &format!("基础形象生成完成, {} bytes", generated.len()));
        fs::write(&base_path, &generated).map_err(|e| format!("保存 base 图片失败: {e}"))?;
        progress.base_image_done = true;
        save_progress_file(&work_dir, &progress)?;
        generated
    } else {
        emit_log(app, task_id, "info", "调用 text_to_image API...");
        let generated = client.text_to_image(&base_prompt, base_size).await?;
        emit_log(app, task_id, "info", &format!("基础形象生成完成, {} bytes", generated.len()));
        fs::write(&base_path, &generated).map_err(|e| format!("保存 base 图片失败: {e}"))?;
        progress.base_image_done = true;
        save_progress_file(&work_dir, &progress)?;
        generated
    };

    emit_progress(
        app,
        &make_progress(task_id, 2, "", "completed", "基础形象已生成"),
    );

    if !has_cached_base {
        // Emit a special event with base image for user confirmation
        let base_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &base_image_bytes,
        );
        let _ = app.emit(
            "pet-creation-base-ready",
            serde_json::json!({
                "taskId": task_id,
                "baseImageB64": base_b64
            }),
        );
    }

    check_cancelled(cancel_token)?;

    // Step 3: Generate animation strips (batched to respect provider ratio limits)
    emit_progress(app, &make_progress(task_id, 3, "", "running", "生成动画帧..."));

    emit_log(app, task_id, "info", &format!("共 {} 行动画条带 (running-left 由镜像生成)", row_count));
    let mut strip_images: Vec<Vec<u8>> = Vec::with_capacity(row_count);
    let mut running_right_bytes: Option<Vec<u8>> = None;

    for i in 0..row_count {
        let state_name = prompt_builder::row_state_name(i);
        let strip_path = work_dir.join(format!("{}.png", state_name));
        check_cancelled(cancel_token)?;

        if progress.completed_strips.iter().any(|s| s == state_name) && strip_path.exists() {
            emit_log(app, task_id, "info", &format!("[{}] 使用缓存条带，跳过 API 生成", state_name));
            let strip_bytes = fs::read(&strip_path).map_err(|e| format!("读取 {} 缓存失败: {e}", state_name))?;
            if state_name == "running-right" {
                running_right_bytes = Some(strip_bytes.clone());
            }
            emit_progress(
                app,
                &make_progress(
                    task_id,
                    3,
                    state_name,
                    "completed",
                    &format!("{} 已复用缓存 ({}/{})", state_name, i + 1, row_count),
                ),
            );
            strip_images.push(strip_bytes);
            continue;
        }

        // running-left: mirror from running-right
        if state_name == "running-left" {
            emit_progress(app, &make_progress(task_id, 3, state_name, "running", "镜像生成..."));
            emit_log(app, task_id, "info", &format!("[{}] 从 running-right 镜像", state_name));

            let right_bytes = running_right_bytes.as_ref()
                .ok_or("running-right 数据不可用，无法镜像")?;
            let right_img = image::load_from_memory(right_bytes)
                .map_err(|e| format!("加载 running-right 失败: {e}"))?;
            let flipped = right_img.fliph();
            let mut buf = std::io::Cursor::new(Vec::new());
            flipped.write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| format!("编码镜像失败: {e}"))?;
            let flipped_bytes = buf.into_inner();

            fs::write(&strip_path, &flipped_bytes)
                .map_err(|e| format!("保存 {} 失败: {e}", state_name))?;
            mark_strip_completed(&mut progress, state_name);
            save_progress_file(&work_dir, &progress)?;
            emit_progress(app, &make_progress(task_id, 3, state_name, "completed", &format!("{} 已完成 (镜像)", state_name)));
            strip_images.push(flipped_bytes);
            continue;
        }

        let total_frames = prompt_builder::row_frame_count(i);
        let max_batch = max_frames_for_provider(&ai_provider);

        // Split into batches: e.g., 6 frames → [3, 3], 8 frames → [4, 4], 4 frames → [4]
        let batches = compute_batches(total_frames, max_batch);
        let num_batches = batches.len();

        emit_progress(app, &make_progress(task_id, 3, state_name, "running",
            &format!("生成 {} ({}/{}) [{}帧, {}批]...", state_name, i + 1, row_count, total_frames, num_batches)));
        emit_log(app, task_id, "info", &format!("[{}] 帧数:{}, 批次:{:?}", state_name, total_frames, batches));

        let guide_data = layout_guides::get(state_name);
        let mut batch_strips: Vec<Vec<u8>> = Vec::new();
        let mut frames_done: u32 = 0;

        for (batch_idx, &batch_frames) in batches.iter().enumerate() {
            check_cancelled(cancel_token)?;

            let batch_width = CELL_WIDTH * batch_frames;
            let batch_size = ImageSize { width: batch_width, height: CELL_HEIGHT };

            let batch_prompt = if num_batches == 1 {
                prompt_builder::build_row_prompt(&prompt_config, i)
            } else {
                prompt_builder::build_batch_prompt(&prompt_config, i, batch_frames, frames_done, total_frames)
            };

            emit_log(app, task_id, "info", &format!("[{}] 批次 {}/{}: {}帧, 尺寸 {}x{}",
                state_name, batch_idx + 1, num_batches, batch_frames, batch_width, CELL_HEIGHT));

            let mut attempt = 0usize;
            let max_attempts = RATE_LIMIT_BACKOFF_SECS.len() + 1;
            let batch_bytes = loop {
                check_cancelled(cancel_token)?;
                attempt += 1;
                let start_time = std::time::Instant::now();
                let result = if let Some(guide) = guide_data {
                    let imgs: Vec<&[u8]> = vec![&base_image_bytes, guide];
                    client.text_and_images_to_image(&batch_prompt, &imgs, batch_size).await
                } else {
                    client.text_and_image_to_image(&batch_prompt, &base_image_bytes, batch_size).await
                };

                match result {
                    Ok(bytes) => {
                        let elapsed = start_time.elapsed().as_secs();
                        emit_log(
                            app,
                            task_id,
                            "info",
                            &format!(
                                "[{}] 批次{} 成功(第{}次), {} bytes, {}s",
                                state_name,
                                batch_idx + 1,
                                attempt,
                                bytes.len(),
                                elapsed
                            ),
                        );
                        break bytes;
                    }
                    Err(e) => {
                        if attempt >= max_attempts {
                            emit_log(
                                app,
                                task_id,
                                "error",
                                &format!(
                                    "[{}] 批次{} 失败(已重试{}次): {}",
                                    state_name,
                                    batch_idx + 1,
                                    attempt - 1,
                                    e
                                ),
                            );
                            return Err(format!("生成 {} 批次{} 失败: {}", state_name, batch_idx + 1, e));
                        }
                        if !is_rate_limit_error(&e) {
                            emit_log(
                                app,
                                task_id,
                                "warn",
                                &format!(
                                    "[{}] 批次{} 第{}次失败(非429)，立即重试: {}",
                                    state_name,
                                    batch_idx + 1,
                                    attempt,
                                    e
                                ),
                            );
                            continue;
                        }

                        let backoff = RATE_LIMIT_BACKOFF_SECS[attempt - 1];
                        emit_log(
                            app,
                            task_id,
                            "warn",
                            &format!(
                                "[{}] 批次{} 第{}次触发限流(429)，{}s 后重试: {}",
                                state_name,
                                batch_idx + 1,
                                attempt,
                                backoff,
                                e
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    }
                }
            };

            batch_strips.push(batch_bytes);
            frames_done += batch_frames;
        }

        // Concatenate batch strips horizontally into one full strip
        let strip_bytes = if batch_strips.len() == 1 {
            batch_strips.into_iter().next().unwrap()
        } else {
            concatenate_strips(&batch_strips)?
        };

        if state_name == "running-right" {
            running_right_bytes = Some(strip_bytes.clone());
        }

        fs::write(&strip_path, &strip_bytes)
            .map_err(|e| format!("保存 {} 失败: {e}", state_name))?;
        mark_strip_completed(&mut progress, state_name);
        save_progress_file(&work_dir, &progress)?;
        emit_progress(app, &make_progress(task_id, 3, state_name, "completed",
            &format!("{} 已完成 ({}/{})", state_name, i + 1, row_count)));
        strip_images.push(strip_bytes);
    }

    emit_progress(app, &make_progress(task_id, 3, "", "completed", "所有动画帧已生成"));

    check_cancelled(cancel_token)?;

    // Step 4: Process images (chroma removal + frame extraction)
    emit_progress(app, &make_progress(task_id, 4, "", "running", "处理图像..."));

    let mut all_rows: Vec<Vec<RgbaImage>> = Vec::with_capacity(row_count);

    for (i, strip_bytes) in strip_images.iter().enumerate() {
        let frame_count = prompt_builder::row_frame_count(i);
        let img = image_processor::load_image_from_bytes(strip_bytes)?;
        let cleaned = image_processor::remove_chroma_key(&img, CHROMA_KEY);
        let frames = image_processor::extract_frames(&cleaned, frame_count);
        all_rows.push(frames);
    }

    emit_progress(app, &make_progress(task_id, 4, "", "completed", "图像处理完成"));

    check_cancelled(cancel_token)?;

    // Step 5: Quality check (basic validation)
    emit_progress(app, &make_progress(task_id, 5, "", "running", "检查帧质量..."));

    for (i, row_frames) in all_rows.iter().enumerate() {
        let state_name = prompt_builder::row_state_name(i);
        let expected = prompt_builder::row_frame_count(i) as usize;
        if row_frames.len() != expected {
            return Err(format!(
                "{} 帧数不匹配: 期望 {}，得到 {}",
                state_name,
                expected,
                row_frames.len()
            ));
        }
    }

    emit_progress(app, &make_progress(task_id, 5, "", "completed", "帧质量检查通过"));

    check_cancelled(cancel_token)?;

    // Step 6: Compose atlas
    emit_progress(app, &make_progress(task_id, 6, "", "running", "合成精灵图集..."));

    let atlas = image_processor::compose_atlas(&all_rows);

    emit_progress(app, &make_progress(task_id, 6, "", "completed", "图集合成完成"));

    check_cancelled(cancel_token)?;

    // Step 7: Package pet
    emit_progress(app, &make_progress(task_id, 7, "", "running", "打包宠物文件..."));

    let pets_dir = root.join("pets").join(&pet_id);
    fs::create_dir_all(&pets_dir).map_err(|e| format!("创建宠物目录失败: {e}"))?;

    let spritesheet_path = pets_dir.join("spritesheet.webp");
    image_processor::save_atlas_webp(&atlas, &spritesheet_path)?;

    let png_path = pets_dir.join("spritesheet.png");
    image_processor::save_atlas_png(&atlas, &png_path)?;

    let manifest = PetManifest {
        id: pet_id.clone(),
        display_name: request.pet_name.clone(),
        description: request.description.clone(),
        spritesheet_path: "spritesheet.webp".into(),
    };
    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("序列化 manifest: {e}"))?;
    fs::write(pets_dir.join("pet.json"), manifest_json)
        .map_err(|e| format!("写入 pet.json 失败: {e}"))?;

    let pet_atlas = PetAtlas {
        cell_width: CELL_WIDTH,
        cell_height: CELL_HEIGHT,
        rows: (0..row_count)
            .map(|i| {
                let state = prompt_builder::row_state_name(i);
                let frames = prompt_builder::row_frame_count(i);
                let display = match state {
                    "idle" => "待机",
                    "running-right" => "向右跑",
                    "running-left" => "向左跑",
                    "waving" => "挥手",
                    "jumping" => "跳跃",
                    "failed" => "失败",
                    "waiting" => "等待",
                    "running" => "专注",
                    "review" => "审视",
                    _ => state,
                };
                PetAtlasRow {
                    state: state.to_string(),
                    display_name: display.to_string(),
                    frames,
                }
            })
            .collect(),
    };
    let atlas_json =
        serde_json::to_string_pretty(&pet_atlas).map_err(|e| format!("序列化 atlas: {e}"))?;
    fs::write(pets_dir.join("pet-atlas.json"), atlas_json)
        .map_err(|e| format!("写入 pet-atlas.json 失败: {e}"))?;

    emit_log(app, task_id, "info", &format!("步骤图已保留在: {}", work_dir.display()));
    emit_progress(app, &make_progress(task_id, 7, "", "completed", "宠物打包完成！"));

    Ok(pet_id)
}

fn check_cancelled(token: &CancellationToken) -> Result<(), String> {
    if token.is_cancelled() {
        return Err("用户取消".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn start_pet_creation(app: AppHandle, request: PetCreationRequest) -> Result<String, String> {
    let ai_config = read_ai_config(&app)?;
    if ai_config.api_key.is_empty() {
        return Err("请先在设置中配置 AI API Key".into());
    }

    let task_id = Uuid::new_v4().to_string();
    let cancel_token = CancellationToken::new();

    let task = ActiveTask {
        cancel_token: cancel_token.clone(),
    };

    {
        let mut map = tasks().lock().await;
        map.insert(task_id.clone(), task);
    }

    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    tokio::spawn(async move {
        run_creation_pipeline(app_clone, task_id_clone, request, cancel_token, None).await;
    });

    Ok(task_id)
}

#[tauri::command]
pub fn list_incomplete_tasks(app: AppHandle) -> Result<Vec<IncompleteCreationTask>, String> {
    let root = ensure_app_layout(&app)?;
    let tmp_dir = root.join("tmp");
    if !tmp_dir.exists() {
        return Ok(Vec::new());
    }

    let mut tasks_list = Vec::new();
    let entries = fs::read_dir(&tmp_dir).map_err(|e| format!("读取 tmp 目录失败: {e}"))?;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !progress_file_path(&path).exists() {
            continue;
        }
        let Ok(progress) = load_progress_file(&path) else {
            continue;
        };
        if progress.completed_strips.len() >= progress.total_rows {
            continue;
        }
        let Some(work_dir_name) = path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        tasks_list.push(IncompleteCreationTask {
            work_dir_name: work_dir_name.to_string(),
            pet_name: progress.request.pet_name,
            base_image_done: progress.base_image_done,
            completed_count: progress.completed_strips.len(),
            total_rows: progress.total_rows,
        });
    }
    tasks_list.sort_by(|a, b| a.work_dir_name.cmp(&b.work_dir_name));
    Ok(tasks_list)
}

#[tauri::command]
pub async fn resume_pet_creation(app: AppHandle, work_dir_name: String) -> Result<String, String> {
    let ai_config = read_ai_config(&app)?;
    if ai_config.api_key.is_empty() {
        return Err("请先在设置中配置 AI API Key".into());
    }

    let root = ensure_app_layout(&app)?;
    let work_dir = root.join("tmp").join(&work_dir_name);
    if !work_dir.exists() {
        return Err("续跑任务目录不存在".into());
    }
    let progress = load_progress_file(&work_dir)?;
    if progress.completed_strips.len() >= progress.total_rows {
        return Err("该任务已全部完成，无需续跑".into());
    }

    let task_id = Uuid::new_v4().to_string();
    let cancel_token = CancellationToken::new();
    let task = ActiveTask {
        cancel_token: cancel_token.clone(),
    };

    {
        let mut map = tasks().lock().await;
        map.insert(task_id.clone(), task);
    }

    let app_clone = app.clone();
    let task_id_clone = task_id.clone();
    let request = progress.request;
    tokio::spawn(async move {
        run_creation_pipeline(
            app_clone,
            task_id_clone,
            request,
            cancel_token,
            Some(work_dir_name),
        )
        .await;
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_pet_creation(_app: AppHandle, task_id: String) -> Result<(), String> {
    let map = tasks().lock().await;
    if let Some(task) = map.get(&task_id) {
        task.cancel_token.cancel();
        Ok(())
    } else {
        Err("任务不存在或已完成".into())
    }
}

#[tauri::command]
pub async fn confirm_base_image(task_id: String, confirmed: bool) -> Result<(), String> {
    if !confirmed {
        let map = tasks().lock().await;
        if let Some(task) = map.get(&task_id) {
            task.cancel_token.cancel();
        }
    }
    Ok(())
}
