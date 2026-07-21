use aipet_core::import_pet_zip_bytes;
use aipet_core::{PetCreationProgress, PetCreationRequest};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config_manager::ensure_app_layout;
use crate::pet_creator::PetCreationResult;
use crate::remote_config::read_remote_config;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerStatus {
    pub online: bool,
    pub state: String,
    pub active_task_id: Option<String>,
    pub pet_name: Option<String>,
    pub phase: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusDto {
    state: String,
    active_task_id: Option<String>,
    pet_name: Option<String>,
    phase: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTaskDto {
    task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteLogEntry {
    time: String,
    level: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskSnapshotDto {
    #[allow(dead_code)]
    task_id: String,
    phase: String,
    progress: Option<PetCreationProgress>,
    logs: Vec<RemoteLogEntry>,
    base_image_b64: Option<String>,
    #[allow(dead_code)]
    pet_id: Option<String>,
    artifact_sha256: Option<String>,
    error: Option<String>,
}

struct RemoteTaskRuntime {
    cancel: CancellationToken,
    remote_task_id: String,
}

static REMOTE_TASKS: std::sync::OnceLock<Arc<Mutex<HashMap<String, RemoteTaskRuntime>>>> =
    std::sync::OnceLock::new();

fn remote_tasks() -> &'static Arc<Mutex<HashMap<String, RemoteTaskRuntime>>> {
    REMOTE_TASKS.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("create http client: {e}"))
}

fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

#[tauri::command]
pub async fn get_remote_server_status(app: AppHandle) -> Result<RemoteServerStatus, String> {
    let cfg = read_remote_config(&app)?;
    let base = normalize_base_url(&cfg.base_url);
    let client = http_client()?;
    match client.get(format!("{base}/status")).send().await {
        Ok(resp) => {
            if !resp.status().is_success() {
                return Ok(RemoteServerStatus {
                    online: false,
                    state: "offline".into(),
                    active_task_id: None,
                    pet_name: None,
                    phase: None,
                    error: Some(format!("status HTTP {}", resp.status())),
                });
            }
            let dto: StatusDto = resp
                .json()
                .await
                .map_err(|e| format!("parse status: {e}"))?;
            Ok(RemoteServerStatus {
                online: true,
                state: dto.state,
                active_task_id: dto.active_task_id,
                pet_name: dto.pet_name,
                phase: dto.phase,
                error: None,
            })
        }
        Err(e) => Ok(RemoteServerStatus {
            online: false,
            state: "offline".into(),
            active_task_id: None,
            pet_name: None,
            phase: None,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn start_remote_pet_creation(
    app: AppHandle,
    request: PetCreationRequest,
) -> Result<String, String> {
    let cfg = read_remote_config(&app)?;
    let base = normalize_base_url(&cfg.base_url);
    let client = http_client()?;

    let resp = client
        .post(format!("{base}/tasks"))
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("连接生成服务失败: {e}"))?;

    if resp.status() == reqwest::StatusCode::CONFLICT {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("服务端忙碌: {text}"));
    }
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("创建远程任务失败 {status}: {text}"));
    }

    let created: CreateTaskDto = resp
        .json()
        .await
        .map_err(|e| format!("解析创建任务响应失败: {e}"))?;
    let local_task_id = Uuid::new_v4().to_string();
    let remote_task_id = created.task_id;
    let cancel = CancellationToken::new();

    {
        let mut map = remote_tasks().lock().await;
        map.insert(
            local_task_id.clone(),
            RemoteTaskRuntime {
                cancel: cancel.clone(),
                remote_task_id: remote_task_id.clone(),
            },
        );
    }

    let app_clone = app.clone();
    let local_id = local_task_id.clone();
    let remote_id = remote_task_id.clone();
    tokio::spawn(async move {
        let result = poll_remote_task(
            app_clone.clone(),
            local_id.clone(),
            remote_id,
            base,
            cancel.clone(),
        )
        .await;
        match result {
            Ok(pet_id) => {
                let _ = app_clone.emit(
                    "pet-creation-completed",
                    PetCreationResult {
                        task_id: local_id.clone(),
                        pet_id,
                    },
                );
            }
            Err(e) => {
                if cancel.is_cancelled() || e.contains("用户取消") || e.contains("cancelled") {
                    let _ = app_clone.emit(
                        "pet-creation-cancelled",
                        serde_json::json!({ "taskId": local_id, "reason": "用户取消" }),
                    );
                } else {
                    let _ = app_clone.emit(
                        "pet-creation-failed",
                        serde_json::json!({ "taskId": local_id, "reason": e }),
                    );
                }
            }
        }
        let mut map = remote_tasks().lock().await;
        map.remove(&local_id);
    });

    Ok(local_task_id)
}

#[tauri::command]
pub async fn cancel_remote_pet_creation(app: AppHandle, task_id: String) -> Result<(), String> {
    let remote_id = {
        let map = remote_tasks().lock().await;
        if let Some(task) = map.get(&task_id) {
            task.cancel.cancel();
            Some(task.remote_task_id.clone())
        } else {
            None
        }
    };

    if let Some(remote_id) = remote_id {
        let cfg = read_remote_config(&app)?;
        let base = normalize_base_url(&cfg.base_url);
        let client = http_client()?;
        let _ = client
            .post(format!("{base}/tasks/{remote_id}/cancel"))
            .send()
            .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn confirm_remote_base_image(
    app: AppHandle,
    remote_task_id: String,
    confirmed: bool,
) -> Result<(), String> {
    let cfg = read_remote_config(&app)?;
    let base = normalize_base_url(&cfg.base_url);
    let client = http_client()?;
    let resp = client
        .post(format!("{base}/tasks/{remote_task_id}/confirm-base"))
        .json(&serde_json::json!({ "confirmed": confirmed }))
        .send()
        .await
        .map_err(|e| format!("确认请求失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("确认失败 {status}: {text}"));
    }
    Ok(())
}

#[tauri::command]
pub fn import_pet_zip_file(app: AppHandle, zip_path: String) -> Result<String, String> {
    let root = ensure_app_layout(&app)?;
    let bytes = std::fs::read(&zip_path).map_err(|e| format!("读取 ZIP 失败: {e}"))?;
    let pets_dir = root.join("pets");
    let staging_parent = root.join("tmp");
    std::fs::create_dir_all(&staging_parent).map_err(|e| format!("create tmp: {e}"))?;
    import_pet_zip_bytes(&bytes, None, &pets_dir, &staging_parent)
}

async fn poll_remote_task(
    app: AppHandle,
    local_task_id: String,
    remote_task_id: String,
    base: String,
    cancel: CancellationToken,
) -> Result<String, String> {
    let client = http_client()?;
    let mut last_log_len = 0usize;
    let mut base_emitted = false;
    let mut last_progress_key = String::new();

    emit_log(
        &app,
        &local_task_id,
        "info",
        &format!("remoteTaskId={remote_task_id}"),
    );
    let _ = app.emit(
        "pet-creation-remote-meta",
        serde_json::json!({
            "taskId": local_task_id,
            "remoteTaskId": remote_task_id,
        }),
    );

    loop {
        if cancel.is_cancelled() {
            let _ = client
                .post(format!("{base}/tasks/{remote_task_id}/cancel"))
                .send()
                .await;
            return Err("用户取消".into());
        }

        let resp = client
            .get(format!("{base}/tasks/{remote_task_id}"))
            .send()
            .await
            .map_err(|e| format!("轮询任务失败: {e}"))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            tokio::time::sleep(Duration::from_millis(800)).await;
            continue;
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("查询任务失败 {status}: {text}"));
        }

        let snap: TaskSnapshotDto = resp
            .json()
            .await
            .map_err(|e| format!("解析任务状态失败: {e}"))?;

        if snap.logs.len() > last_log_len {
            for entry in &snap.logs[last_log_len..] {
                let _ = app.emit(
                    "pet-creation-log",
                    serde_json::json!({
                        "taskId": local_task_id,
                        "time": entry.time,
                        "level": entry.level,
                        "message": entry.message,
                    }),
                );
            }
            last_log_len = snap.logs.len();
        }

        if let Some(progress) = snap.progress {
            let key = format!(
                "{}:{}:{}:{}",
                progress.step, progress.sub_step, progress.status, progress.message
            );
            if key != last_progress_key {
                last_progress_key = key;
                let mut p = progress;
                p.task_id = local_task_id.clone();
                let _ = app.emit("pet-creation-progress", p);
            }
        }

        if !base_emitted {
            if let Some(b64) = snap.base_image_b64 {
                if !b64.is_empty() {
                    base_emitted = true;
                    let _ = app.emit(
                        "pet-creation-base-ready",
                        serde_json::json!({
                            "taskId": local_task_id,
                            "baseImageB64": b64,
                            "remoteTaskId": remote_task_id,
                        }),
                    );
                }
            }
        }

        match snap.phase.as_str() {
            "succeeded" => {
                let sha = snap.artifact_sha256;
                return download_and_import(&app, &base, &remote_task_id, sha.as_deref()).await;
            }
            "failed" => {
                return Err(snap.error.unwrap_or_else(|| "远程生成失败".into()));
            }
            "cancelled" => {
                return Err("用户取消".into());
            }
            _ => {
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        }
    }
}

async fn download_and_import(
    app: &AppHandle,
    base: &str,
    remote_task_id: &str,
    expected_sha: Option<&str>,
) -> Result<String, String> {
    let client = http_client()?;
    let resp = client
        .get(format!("{base}/tasks/{remote_task_id}/artifact"))
        .timeout(Duration::from_secs(180))
        .send()
        .await
        .map_err(|e| format!("下载产物失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("下载产物失败 {status}: {text}"));
    }

    let header_sha = resp
        .headers()
        .get("x-aipet-sha256")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let sha = expected_sha.map(|s| s.to_string()).or(header_sha);

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取产物失败: {e}"))?;

    let root = ensure_app_layout(app)?;
    let pets_dir = root.join("pets");
    let staging_parent = root.join("tmp");
    std::fs::create_dir_all(&staging_parent).map_err(|e| format!("create tmp: {e}"))?;

    import_pet_zip_bytes(&bytes, sha.as_deref(), &pets_dir, &staging_parent)
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
