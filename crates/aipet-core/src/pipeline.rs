use async_trait::async_trait;
use base64::Engine;
use image::RgbaImage;
use std::fs;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

use crate::ai_api_client::{AiApiClient, ImageSize};
use crate::ai_config::AiApiConfig;
use crate::image_processor;
use crate::layout_guides;
use crate::package::{self, PetPackageFiles};
use crate::prompt_builder::{self, PetPromptConfig};
use crate::types::{
    sanitize_pet_id, CreationProgressFile, PetAtlas, PetAtlasRow, PetCreationProgress,
    PetCreationRequest, PetManifest, CELL_HEIGHT, CELL_WIDTH, CHROMA_KEY,
};

const RATE_LIMIT_BACKOFF_SECS: [u64; 3] = [5, 15, 30];

#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn on_progress(&self, progress: PetCreationProgress);
    async fn on_log(&self, task_id: &str, level: &str, msg: &str);
    async fn on_base_ready(&self, task_id: &str, base_image_b64: &str);
    async fn wait_base_confirm(&self, task_id: &str) -> Result<bool, String>;
}

#[derive(Debug, Clone)]
pub struct PipelineOptions {
    pub work_dir: PathBuf,
    pub pets_dir: PathBuf,
    pub write_zip: bool,
    pub zip_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub pet_id: String,
    pub pet_dir: PathBuf,
    pub zip_path: Option<PathBuf>,
    pub zip_sha256: Option<String>,
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
    let data = fs::read_to_string(&path).map_err(|e| format!("read progress: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse progress: {e}"))
}

fn save_progress_file(work_dir: &Path, progress: &CreationProgressFile) -> Result<(), String> {
    let path = progress_file_path(work_dir);
    let data = serde_json::to_string_pretty(progress).map_err(|e| format!("serialize progress: {e}"))?;
    fs::write(path, data).map_err(|e| format!("write progress: {e}"))
}

fn mark_strip_completed(progress: &mut CreationProgressFile, state_name: &str) {
    if progress.completed_strips.iter().any(|s| s == state_name) {
        return;
    }
    progress.completed_strips.push(state_name.to_string());
}

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

fn max_frames_for_provider(provider: &str) -> u32 {
    if provider == "openai" {
        return 3;
    }
    4
}

fn is_rate_limit_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("429")
        || normalized.contains("too many requests")
        || normalized.contains("engineoverloaded")
}

fn concatenate_strips(strips: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let images: Vec<image::DynamicImage> = strips
        .iter()
        .map(|b| image::load_from_memory(b).map_err(|e| format!("load batch image: {e}")))
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
        .map_err(|e| format!("encode strip: {e}"))?;
    Ok(buf.into_inner())
}

fn check_cancelled(token: &CancellationToken) -> Result<(), String> {
    if token.is_cancelled() {
        return Err("cancelled".into());
    }
    Ok(())
}

fn build_pet_atlas(row_count: usize) -> PetAtlas {
    PetAtlas {
        cell_width: CELL_WIDTH,
        cell_height: CELL_HEIGHT,
        rows: (0..row_count)
            .map(|i| {
                let state = prompt_builder::row_state_name(i);
                let frames = prompt_builder::row_frame_count(i);
                let display = match state {
                    "idle" => "\u{5f85}\u{673a}",
                    "running-right" => "\u{5411}\u{53f3}\u{8dd1}",
                    "running-left" => "\u{5411}\u{5de6}\u{8dd1}",
                    "waving" => "\u{6325}\u{624b}",
                    "jumping" => "\u{8df3}\u{8dc3}",
                    "failed" => "\u{5931}\u{8d25}",
                    "waiting" => "\u{7b49}\u{5f85}",
                    "running" => "\u{4e13}\u{6ce8}",
                    "review" => "\u{5ba1}\u{89c6}",
                    _ => state,
                };
                PetAtlasRow {
                    state: state.to_string(),
                    display_name: display.to_string(),
                    frames,
                }
            })
            .collect(),
    }
}

pub async fn run_pipeline(
    task_id: &str,
    request: &PetCreationRequest,
    ai_config: AiApiConfig,
    options: PipelineOptions,
    cancel_token: &CancellationToken,
    sink: &dyn ProgressSink,
) -> Result<PipelineResult, String> {
    sink.on_progress(make_progress(task_id, 1, "", "running", "prepare workdir"))
        .await;

    let ai_provider = ai_config.provider.clone();
    let client = AiApiClient::new(ai_config)?;

    fs::create_dir_all(&options.work_dir).map_err(|e| format!("create workdir: {e}"))?;
    fs::create_dir_all(&options.pets_dir).map_err(|e| format!("create pets dir: {e}"))?;

    let pet_id = sanitize_pet_id(&request.pet_name);
    let row_count = prompt_builder::row_count();
    let mut progress = if progress_file_path(&options.work_dir).exists() {
        load_progress_file(&options.work_dir)?
    } else {
        create_initial_progress(task_id, request, row_count)
    };
    progress.task_id = task_id.to_string();
    progress.request = request.clone();
    progress.total_rows = row_count;
    save_progress_file(&options.work_dir, &progress)?;

    let prompt_config = PetPromptConfig {
        pet_name: request.pet_name.clone(),
        description: request.description.clone(),
        style_preset: request.style_preset.clone(),
        chroma_key: CHROMA_KEY.into(),
    };

    sink.on_progress(make_progress(task_id, 1, "", "completed", "workdir ready"))
        .await;
    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 2, "", "running", "generate base image"))
        .await;

    let base_prompt = prompt_builder::build_base_prompt(&prompt_config);
    let base_size = ImageSize {
        width: CELL_WIDTH,
        height: CELL_HEIGHT,
    };
    let base_path = options.work_dir.join("base.png");
    let has_cached_base = base_path.exists();

    sink.on_log(
        task_id,
        "info",
        &format!(
            "base prompt: {}",
            &base_prompt[..base_prompt.len().min(200)]
        ),
    )
    .await;
    sink.on_log(
        task_id,
        "info",
        &format!("size: {}x{}", base_size.width, base_size.height),
    )
    .await;

    let base_image_bytes = if has_cached_base {
        sink.on_log(task_id, "info", "reuse cached base image").await;
        let cached = fs::read(&base_path).map_err(|e| format!("read cached base: {e}"))?;
        if !progress.base_image_done {
            progress.base_image_done = true;
            save_progress_file(&options.work_dir, &progress)?;
        }
        cached
    } else if let Some(ref_img_b64) = &request.reference_image {
        let generated = if ref_img_b64.is_empty() {
            sink.on_log(task_id, "info", "call text_to_image").await;
            client.text_to_image(&base_prompt, base_size).await?
        } else {
            let ref_bytes = base64::engine::general_purpose::STANDARD
                .decode(ref_img_b64)
                .map_err(|e| format!("decode reference image: {e}"))?;
            sink.on_log(
                task_id,
                "info",
                &format!("call text_and_image_to_image ({} bytes)", ref_bytes.len()),
            )
            .await;
            client
                .text_and_image_to_image(&base_prompt, &ref_bytes, base_size)
                .await?
        };
        sink.on_log(
            task_id,
            "info",
            &format!("base image done, {} bytes", generated.len()),
        )
        .await;
        fs::write(&base_path, &generated).map_err(|e| format!("save base: {e}"))?;
        progress.base_image_done = true;
        save_progress_file(&options.work_dir, &progress)?;
        generated
    } else {
        sink.on_log(task_id, "info", "call text_to_image").await;
        let generated = client.text_to_image(&base_prompt, base_size).await?;
        sink.on_log(
            task_id,
            "info",
            &format!("base image done, {} bytes", generated.len()),
        )
        .await;
        fs::write(&base_path, &generated).map_err(|e| format!("save base: {e}"))?;
        progress.base_image_done = true;
        save_progress_file(&options.work_dir, &progress)?;
        generated
    };

    sink.on_progress(make_progress(task_id, 2, "", "completed", "base image ready"))
        .await;

    if !has_cached_base {
        let base_b64 = base64::engine::general_purpose::STANDARD.encode(&base_image_bytes);
        sink.on_base_ready(task_id, &base_b64).await;
        sink.on_log(task_id, "info", "waiting for base image confirmation")
            .await;
        let confirmed = sink.wait_base_confirm(task_id).await?;
        if !confirmed {
            cancel_token.cancel();
            return Err("\u{7528}\u{6237}\u{53d6}\u{6d88}".into());
        }
        check_cancelled(cancel_token)?;
        sink.on_log(task_id, "info", "base image confirmed, continue strips")
            .await;
    }

    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 3, "", "running", "generate strips"))
        .await;
    sink.on_log(
        task_id,
        "info",
        &format!("{row_count} strips (running-left mirrored)"),
    )
    .await;

    let mut strip_images: Vec<Vec<u8>> = Vec::with_capacity(row_count);
    let mut running_right_bytes: Option<Vec<u8>> = None;

    for i in 0..row_count {
        let state_name = prompt_builder::row_state_name(i);
        let strip_path = options.work_dir.join(format!("{state_name}.png"));
        check_cancelled(cancel_token)?;

        if progress.completed_strips.iter().any(|s| s == state_name) && strip_path.exists() {
            sink.on_log(task_id, "info", &format!("[{state_name}] reuse cached strip"))
                .await;
            let strip_bytes =
                fs::read(&strip_path).map_err(|e| format!("read {state_name} cache: {e}"))?;
            if state_name == "running-right" {
                running_right_bytes = Some(strip_bytes.clone());
            }
            sink.on_progress(make_progress(
                task_id,
                3,
                state_name,
                "completed",
                &format!("{state_name} cached ({}/{row_count})", i + 1),
            ))
            .await;
            strip_images.push(strip_bytes);
            continue;
        }

        if state_name == "running-left" {
            sink.on_progress(make_progress(task_id, 3, state_name, "running", "mirror"))
                .await;
            sink.on_log(task_id, "info", &format!("[{state_name}] mirror running-right"))
                .await;

            let right_bytes = running_right_bytes
                .as_ref()
                .ok_or("running-right missing for mirror")?;
            let right_img = image::load_from_memory(right_bytes)
                .map_err(|e| format!("load running-right: {e}"))?;
            let flipped = right_img.fliph();
            let mut buf = std::io::Cursor::new(Vec::new());
            flipped
                .write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| format!("encode mirror: {e}"))?;
            let flipped_bytes = buf.into_inner();

            fs::write(&strip_path, &flipped_bytes)
                .map_err(|e| format!("save {state_name}: {e}"))?;
            mark_strip_completed(&mut progress, state_name);
            save_progress_file(&options.work_dir, &progress)?;
            sink.on_progress(make_progress(
                task_id,
                3,
                state_name,
                "completed",
                &format!("{state_name} mirrored"),
            ))
            .await;
            strip_images.push(flipped_bytes);
            continue;
        }

        let total_frames = prompt_builder::row_frame_count(i);
        let max_batch = max_frames_for_provider(&ai_provider);
        let batches = compute_batches(total_frames, max_batch);
        let num_batches = batches.len();

        sink.on_progress(make_progress(
            task_id,
            3,
            state_name,
            "running",
            &format!(
                "gen {state_name} ({}/{row_count}) [{total_frames}f/{num_batches}b]",
                i + 1
            ),
        ))
        .await;
        sink.on_log(
            task_id,
            "info",
            &format!("[{state_name}] frames:{total_frames}, batches:{batches:?}"),
        )
        .await;

        let guide_data = layout_guides::get(state_name);
        let mut batch_strips: Vec<Vec<u8>> = Vec::new();
        let mut frames_done: u32 = 0;

        for (batch_idx, &batch_frames) in batches.iter().enumerate() {
            check_cancelled(cancel_token)?;

            let batch_width = CELL_WIDTH * batch_frames;
            let batch_size = ImageSize {
                width: batch_width,
                height: CELL_HEIGHT,
            };

            let batch_prompt = if num_batches == 1 {
                prompt_builder::build_row_prompt(&prompt_config, i)
            } else {
                prompt_builder::build_batch_prompt(
                    &prompt_config,
                    i,
                    batch_frames,
                    frames_done,
                    total_frames,
                )
            };

            sink.on_log(
                task_id,
                "info",
                &format!(
                    "[{state_name}] batch {}/{num_batches}: {batch_frames}f {batch_width}x{CELL_HEIGHT}",
                    batch_idx + 1
                ),
            )
            .await;

            let mut attempt = 0usize;
            let max_attempts = RATE_LIMIT_BACKOFF_SECS.len() + 1;
            let batch_bytes = loop {
                check_cancelled(cancel_token)?;
                attempt += 1;
                let start_time = std::time::Instant::now();
                let result = if let Some(guide) = guide_data {
                    let imgs: Vec<&[u8]> = vec![&base_image_bytes, guide];
                    client
                        .text_and_images_to_image(&batch_prompt, &imgs, batch_size)
                        .await
                } else {
                    client
                        .text_and_image_to_image(&batch_prompt, &base_image_bytes, batch_size)
                        .await
                };

                match result {
                    Ok(bytes) => {
                        let elapsed = start_time.elapsed().as_secs();
                        sink.on_log(
                            task_id,
                            "info",
                            &format!(
                                "[{state_name}] batch{} ok(try{attempt}), {} bytes, {elapsed}s",
                                batch_idx + 1,
                                bytes.len()
                            ),
                        )
                        .await;
                        break bytes;
                    }
                    Err(e) => {
                        if attempt >= max_attempts {
                            sink.on_log(
                                task_id,
                                "error",
                                &format!(
                                    "[{state_name}] batch{} failed after {} retries: {e}",
                                    batch_idx + 1,
                                    attempt - 1
                                ),
                            )
                            .await;
                            return Err(format!(
                                "generate {state_name} batch{} failed: {e}",
                                batch_idx + 1
                            ));
                        }
                        if !is_rate_limit_error(&e) {
                            sink.on_log(
                                task_id,
                                "warn",
                                &format!(
                                    "[{state_name}] batch{} try{attempt} failed(non-429), retry: {e}",
                                    batch_idx + 1
                                ),
                            )
                            .await;
                            continue;
                        }

                        let backoff = RATE_LIMIT_BACKOFF_SECS[attempt - 1];
                        sink.on_log(
                            task_id,
                            "warn",
                            &format!(
                                "[{state_name}] batch{} try{attempt} rate-limit, wait {backoff}s: {e}",
                                batch_idx + 1
                            ),
                        )
                        .await;
                        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    }
                }
            };

            batch_strips.push(batch_bytes);
            frames_done += batch_frames;
        }

        let strip_bytes = if batch_strips.len() == 1 {
            batch_strips.into_iter().next().unwrap()
        } else {
            concatenate_strips(&batch_strips)?
        };

        if state_name == "running-right" {
            running_right_bytes = Some(strip_bytes.clone());
        }

        fs::write(&strip_path, &strip_bytes).map_err(|e| format!("save {state_name}: {e}"))?;
        mark_strip_completed(&mut progress, state_name);
        save_progress_file(&options.work_dir, &progress)?;
        sink.on_progress(make_progress(
            task_id,
            3,
            state_name,
            "completed",
            &format!("{state_name} done ({}/{row_count})", i + 1),
        ))
        .await;
        strip_images.push(strip_bytes);
    }

    sink.on_progress(make_progress(task_id, 3, "", "completed", "all strips done"))
        .await;
    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 4, "", "running", "process images"))
        .await;
    let mut all_rows: Vec<Vec<RgbaImage>> = Vec::with_capacity(row_count);
    for (i, strip_bytes) in strip_images.iter().enumerate() {
        let frame_count = prompt_builder::row_frame_count(i);
        let img = image_processor::load_image_from_bytes(strip_bytes)?;
        let cleaned = image_processor::remove_chroma_key(&img, CHROMA_KEY);
        let frames = image_processor::extract_frames(&cleaned, frame_count);
        all_rows.push(frames);
    }
    sink.on_progress(make_progress(task_id, 4, "", "completed", "images processed"))
        .await;
    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 5, "", "running", "quality check"))
        .await;
    for (i, row_frames) in all_rows.iter().enumerate() {
        let state_name = prompt_builder::row_state_name(i);
        let expected = prompt_builder::row_frame_count(i) as usize;
        if row_frames.len() != expected {
            return Err(format!(
                "{state_name} frame count mismatch: expect {expected}, got {}",
                row_frames.len()
            ));
        }
    }
    sink.on_progress(make_progress(task_id, 5, "", "completed", "quality ok"))
        .await;
    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 6, "", "running", "compose atlas"))
        .await;
    let atlas = image_processor::compose_atlas(&all_rows);
    sink.on_progress(make_progress(task_id, 6, "", "completed", "atlas ready"))
        .await;
    check_cancelled(cancel_token)?;

    sink.on_progress(make_progress(task_id, 7, "", "running", "package pet"))
        .await;

    let pet_dir = options.pets_dir.join(&pet_id);
    fs::create_dir_all(&pet_dir).map_err(|e| format!("create pet dir: {e}"))?;

    let spritesheet_path = pet_dir.join("spritesheet.webp");
    image_processor::save_atlas_webp(&atlas, &spritesheet_path)?;
    let png_path = pet_dir.join("spritesheet.png");
    image_processor::save_atlas_png(&atlas, &png_path)?;

    let manifest = PetManifest {
        id: pet_id.clone(),
        display_name: request.pet_name.clone(),
        description: request.description.clone(),
        spritesheet_path: "spritesheet.webp".into(),
    };
    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| format!("serialize manifest: {e}"))?;
    fs::write(pet_dir.join("pet.json"), manifest_json)
        .map_err(|e| format!("write pet.json: {e}"))?;

    let pet_atlas = build_pet_atlas(row_count);
    let atlas_json =
        serde_json::to_string_pretty(&pet_atlas).map_err(|e| format!("serialize atlas: {e}"))?;
    fs::write(pet_dir.join("pet-atlas.json"), atlas_json)
        .map_err(|e| format!("write pet-atlas.json: {e}"))?;

    let mut zip_path = None;
    let mut zip_sha256 = None;
    if options.write_zip {
        let target = options
            .zip_path
            .clone()
            .unwrap_or_else(|| options.work_dir.join(format!("{pet_id}.zip")));
        let files = PetPackageFiles {
            pet_json: fs::read(pet_dir.join("pet.json")).map_err(|e| format!("read pet.json: {e}"))?,
            pet_atlas_json: fs::read(pet_dir.join("pet-atlas.json"))
                .map_err(|e| format!("read pet-atlas.json: {e}"))?,
            spritesheet_webp: fs::read(&spritesheet_path)
                .map_err(|e| format!("read spritesheet.webp: {e}"))?,
            spritesheet_png: Some(
                fs::read(&png_path).map_err(|e| format!("read spritesheet.png: {e}"))?,
            ),
        };
        let sha = package::write_pet_zip(&target, &files)?;
        zip_sha256 = Some(sha);
        zip_path = Some(target);
    }

    sink.on_log(
        task_id,
        "info",
        &format!("workdir kept at: {}", options.work_dir.display()),
    )
    .await;
    sink.on_progress(make_progress(task_id, 7, "", "completed", "package done"))
        .await;

    Ok(PipelineResult {
        pet_id,
        pet_dir,
        zip_path,
        zip_sha256,
    })
}
