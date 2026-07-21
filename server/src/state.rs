use aipet_core::{
    run_pipeline, AiApiConfig, PetCreationProgress, PetCreationRequest, PipelineOptions,
    ProgressSink,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::audit::{AuditStore, ClientInfo, RunRecord};

const BASE_CONFIRM_TIMEOUT_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServerState {
    Idle,
    Busy,
    AwaitingConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPhase {
    Running,
    AwaitingConfirmation,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub time: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task_id: String,
    pub pet_name: String,
    pub phase: TaskPhase,
    pub started_at: DateTime<Utc>,
    pub progress: Option<PetCreationProgress>,
    pub logs: Vec<LogEntry>,
    pub base_image_b64: Option<String>,
    pub pet_id: Option<String>,
    pub artifact_sha256: Option<String>,
    pub error: Option<String>,
    pub client_ip: Option<String>,
}

struct ActiveTaskInner {
    snapshot: TaskSnapshot,
    cancel: CancellationToken,
    confirm_notify: Arc<Notify>,
    confirmed: Option<bool>,
    zip_path: PathBuf,
    record: RunRecord,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub ai_config_path: PathBuf,
    pub audit: Arc<AuditStore>,
    active: RwLock<Option<Arc<Mutex<ActiveTaskInner>>>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, ai_config_path: PathBuf) -> Result<Self, String> {
        let audit = Arc::new(AuditStore::new(data_dir.clone())?);
        Ok(Self {
            data_dir,
            ai_config_path,
            audit,
            active: RwLock::new(None),
        })
    }

    pub fn load_ai_config(&self) -> Result<AiApiConfig, String> {
        if !self.ai_config_path.exists() {
            return Err(format!(
                "AI 配置不存在: {}。请复制 config.example.json 为 ai-config.json 并填写 API Key",
                self.ai_config_path.display()
            ));
        }
        let data = std::fs::read_to_string(&self.ai_config_path)
            .map_err(|e| format!("读取 AI 配置失败: {e}"))?;
        let cfg: AiApiConfig =
            serde_json::from_str(&data).map_err(|e| format!("解析 AI 配置失败: {e}"))?;
        let cfg = cfg.normalize_cache();
        if cfg.api_key.is_empty() {
            return Err("AI API Key 未配置".into());
        }
        Ok(cfg)
    }

    pub async fn status(&self) -> StatusResponse {
        let guard = self.active.read().await;
        let Some(active) = guard.as_ref() else {
            return StatusResponse {
                state: ServerState::Idle,
                active_task_id: None,
                pet_name: None,
                started_at: None,
                phase: None,
                client_ip: None,
            };
        };
        let task = active.lock().await;
        let state = match task.snapshot.phase {
            TaskPhase::AwaitingConfirmation => ServerState::AwaitingConfirmation,
            TaskPhase::Running => ServerState::Busy,
            TaskPhase::Succeeded | TaskPhase::Failed | TaskPhase::Cancelled => ServerState::Idle,
        };
        StatusResponse {
            state,
            active_task_id: Some(task.snapshot.task_id.clone()),
            pet_name: Some(task.snapshot.pet_name.clone()),
            started_at: Some(task.snapshot.started_at),
            phase: Some(task.snapshot.phase.clone()),
            client_ip: task.snapshot.client_ip.clone(),
        }
    }

    pub async fn get_task(&self, task_id: &str) -> Option<TaskSnapshot> {
        let guard = self.active.read().await;
        if let Some(active) = guard.as_ref() {
            let task = active.lock().await;
            if task.snapshot.task_id == task_id {
                return Some(task.snapshot.clone());
            }
        }
        let record = self.audit.read_run_meta(task_id)?;
        Some(TaskSnapshot {
            task_id: record.task_id,
            pet_name: record.request.pet_name,
            phase: match record.status.as_str() {
                "succeeded" => TaskPhase::Succeeded,
                "cancelled" => TaskPhase::Cancelled,
                "failed" => TaskPhase::Failed,
                "awaiting_confirmation" => TaskPhase::AwaitingConfirmation,
                _ => TaskPhase::Running,
            },
            started_at: record.started_at,
            progress: None,
            logs: Vec::new(),
            base_image_b64: None,
            pet_id: record.pet_id,
            artifact_sha256: record.artifact_sha256,
            error: record.error,
            client_ip: Some(record.client.ip),
        })
    }

    pub async fn start_task(
        self: &Arc<Self>,
        request: PetCreationRequest,
        client: ClientInfo,
    ) -> Result<String, StartError> {
        {
            let guard = self.active.read().await;
            if let Some(active) = guard.as_ref() {
                let task = active.lock().await;
                match task.snapshot.phase {
                    TaskPhase::Running | TaskPhase::AwaitingConfirmation => {
                        return Err(StartError::Busy {
                            task_id: task.snapshot.task_id.clone(),
                            pet_name: task.snapshot.pet_name.clone(),
                        });
                    }
                    TaskPhase::Succeeded | TaskPhase::Failed | TaskPhase::Cancelled => {}
                }
            }
        }

        let ai_config = self.load_ai_config().map_err(StartError::Config)?;
        let task_id = Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        let confirm_notify = Arc::new(Notify::new());

        let (run_dir, work_dir, pets_dir, record) = self
            .audit
            .prepare_run(&task_id, &request, &client)
            .map_err(StartError::Config)?;
        let zip_path = run_dir.join("artifact.zip");

        let snapshot = TaskSnapshot {
            task_id: task_id.clone(),
            pet_name: request.pet_name.clone(),
            phase: TaskPhase::Running,
            started_at: Utc::now(),
            progress: None,
            logs: Vec::new(),
            base_image_b64: None,
            pet_id: None,
            artifact_sha256: None,
            error: None,
            client_ip: Some(client.ip.clone()),
        };

        let inner = Arc::new(Mutex::new(ActiveTaskInner {
            snapshot,
            cancel: cancel.clone(),
            confirm_notify: confirm_notify.clone(),
            confirmed: None,
            zip_path: zip_path.clone(),
            record,
        }));

        {
            let mut guard = self.active.write().await;
            *guard = Some(inner.clone());
        }

        let state = Arc::clone(self);
        let request_clone = request.clone();
        let task_id_for_run = task_id.clone();
        let audit = Arc::clone(&self.audit);
        tokio::spawn(async move {
            let sink = ServerSink {
                task: Arc::clone(&inner),
                audit: Arc::clone(&audit),
            };
            let options = PipelineOptions {
                work_dir,
                pets_dir,
                write_zip: true,
                zip_path: Some(zip_path.clone()),
            };

            let result = run_pipeline(
                &task_id_for_run,
                &request_clone,
                ai_config,
                options,
                &cancel,
                &sink,
            )
            .await;

            let mut task = inner.lock().await;
            let (status, pet_id, sha, error, pet_dir, artifact) = match result {
                Ok(pipeline) => {
                    task.snapshot.phase = TaskPhase::Succeeded;
                    task.snapshot.pet_id = Some(pipeline.pet_id.clone());
                    task.snapshot.artifact_sha256 = pipeline.zip_sha256.clone();
                    push_log(
                        &mut task,
                        &audit,
                        "info",
                        &format!(
                            "任务完成 pet_id={} zip={}",
                            pipeline.pet_id,
                            pipeline
                                .zip_path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "-".into())
                        ),
                    );
                    (
                        "succeeded",
                        Some(pipeline.pet_id.clone()),
                        pipeline.zip_sha256.clone(),
                        None,
                        Some(pipeline.pet_dir),
                        pipeline.zip_path,
                    )
                }
                Err(e) => {
                    if cancel.is_cancelled() || e.contains("用户取消") || e.contains("cancelled") {
                        task.snapshot.phase = TaskPhase::Cancelled;
                        task.snapshot.error = Some("用户取消".into());
                        push_log(&mut task, &audit, "warn", "任务已取消");
                        ("cancelled", None, None, Some("用户取消".into()), None, None)
                    } else {
                        task.snapshot.phase = TaskPhase::Failed;
                        task.snapshot.error = Some(e.clone());
                        push_log(&mut task, &audit, "error", &e);
                        ("failed", None, None, Some(e), None, None)
                    }
                }
            };

            let record = task.record.clone();
            drop(task);
            if let Err(e) =
                audit.finalize_run(record, status, pet_id, sha, error, pet_dir, artifact)
            {
                audit.server_log_line("error", &format!("[{task_id_for_run}] finalize failed: {e}"));
            }
            // Keep last finished task in memory for polling; state already Idle via phase.
            let _ = state;
        });

        Ok(task_id)
    }

    pub async fn confirm_base(&self, task_id: &str, confirmed: bool) -> Result<(), String> {
        let guard = self.active.read().await;
        let active = guard.as_ref().ok_or("任务不存在或已完成")?;
        let mut task = active.lock().await;
        if task.snapshot.task_id != task_id {
            return Err("任务不存在或已完成".into());
        }
        if task.snapshot.phase != TaskPhase::AwaitingConfirmation {
            return Err("当前不在等待基础形象确认状态".into());
        }
        if !confirmed {
            task.cancel.cancel();
        }
        task.confirmed = Some(confirmed);
        task.confirm_notify.notify_one();
        if confirmed {
            task.snapshot.phase = TaskPhase::Running;
            push_log(&mut task, &self.audit, "info", "用户已确认基础形象");
        } else {
            task.snapshot.phase = TaskPhase::Cancelled;
            push_log(&mut task, &self.audit, "warn", "用户拒绝基础形象");
        }
        Ok(())
    }

    pub async fn cancel_task(&self, task_id: &str) -> Result<(), String> {
        let guard = self.active.read().await;
        let active = guard.as_ref().ok_or("任务不存在或已完成")?;
        let mut task = active.lock().await;
        if task.snapshot.task_id != task_id {
            return Err("任务不存在或已完成".into());
        }
        task.cancel.cancel();
        task.confirmed = Some(false);
        task.confirm_notify.notify_one();
        push_log(&mut task, &self.audit, "warn", "收到取消请求");
        Ok(())
    }

    pub async fn artifact_path(&self, task_id: &str) -> Result<(PathBuf, Option<String>), String> {
        let guard = self.active.read().await;
        if let Some(active) = guard.as_ref() {
            let task = active.lock().await;
            if task.snapshot.task_id == task_id {
                if task.snapshot.phase != TaskPhase::Succeeded {
                    return Err("任务尚未成功完成".into());
                }
                if task.zip_path.exists() {
                    return Ok((task.zip_path.clone(), task.snapshot.artifact_sha256.clone()));
                }
            }
        }
        let run_zip = self.data_dir.join("runs").join(task_id).join("artifact.zip");
        if run_zip.exists() {
            let sha = self
                .audit
                .read_run_meta(task_id)
                .and_then(|r| r.artifact_sha256);
            return Ok((run_zip, sha));
        }
        let path = self.data_dir.join("artifacts").join(format!("{task_id}.zip"));
        if path.exists() {
            let sha = self
                .audit
                .read_run_meta(task_id)
                .and_then(|r| r.artifact_sha256);
            return Ok((path, sha));
        }
        Err("任务不存在或产物不可用".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub state: ServerState,
    pub active_task_id: Option<String>,
    pub pet_name: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub phase: Option<TaskPhase>,
    pub client_ip: Option<String>,
}

pub enum StartError {
    Busy { task_id: String, pet_name: String },
    Config(String),
}

fn push_log(task: &mut ActiveTaskInner, audit: &AuditStore, level: &str, message: &str) {
    let time = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
    task.snapshot.logs.push(LogEntry {
        time,
        level: level.to_string(),
        message: message.to_string(),
    });
    if task.snapshot.logs.len() > 500 {
        let overflow = task.snapshot.logs.len() - 500;
        task.snapshot.logs.drain(0..overflow);
    }
    audit.append_task_log(&task.snapshot.task_id, level, message);
}

struct ServerSink {
    task: Arc<Mutex<ActiveTaskInner>>,
    audit: Arc<AuditStore>,
}

#[async_trait]
impl ProgressSink for ServerSink {
    async fn on_progress(&self, progress: PetCreationProgress) {
        let mut task = self.task.lock().await;
        let msg = format!(
            "progress step={} sub={} status={} msg={}",
            progress.step, progress.sub_step, progress.status, progress.message
        );
        task.snapshot.progress = Some(progress);
        push_log(&mut task, &self.audit, "info", &msg);
    }

    async fn on_log(&self, _task_id: &str, level: &str, msg: &str) {
        let mut task = self.task.lock().await;
        push_log(&mut task, &self.audit, level, msg);
    }

    async fn on_base_ready(&self, _task_id: &str, base_image_b64: &str) {
        let mut task = self.task.lock().await;
        task.snapshot.base_image_b64 = Some(base_image_b64.to_string());
        task.snapshot.phase = TaskPhase::AwaitingConfirmation;
        task.confirmed = None;
        let run_dir = self.audit.run_dir(&task.snapshot.task_id);
        if let Ok(bytes) =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base_image_b64)
        {
            let _ = std::fs::write(run_dir.join("base-preview.png"), bytes);
        }
        push_log(&mut task, &self.audit, "info", "基础形象已生成，等待确认");
    }

    async fn wait_base_confirm(&self, _task_id: &str) -> Result<bool, String> {
        let notify = {
            let task = self.task.lock().await;
            Arc::clone(&task.confirm_notify)
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(BASE_CONFIRM_TIMEOUT_SECS),
            async {
                loop {
                    {
                        let task = self.task.lock().await;
                        if let Some(v) = task.confirmed {
                            return Ok::<bool, String>(v);
                        }
                        if task.cancel.is_cancelled() {
                            return Ok(false);
                        }
                    }
                    notify.notified().await;
                }
            },
        )
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => {
                let mut task = self.task.lock().await;
                task.cancel.cancel();
                task.snapshot.phase = TaskPhase::Cancelled;
                task.snapshot.error = Some("基础形象确认超时".into());
                push_log(&mut task, &self.audit, "error", "基础形象确认超时（30分钟）");
                Err("基础形象确认超时".into())
            }
        }
    }
}
