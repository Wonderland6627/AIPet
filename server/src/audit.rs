use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aipet_core::PetCreationRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub ip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forwarded_for: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequestSummary {
    pub pet_name: String,
    pub description: String,
    pub style_preset: String,
    pub has_reference_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub task_id: String,
    pub status: String,
    pub client: ClientInfo,
    pub request: RunRequestSummary,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub run_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pet_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_path: Option<String>,
}

pub struct AuditStore {
    data_dir: PathBuf,
    server_log: Mutex<File>,
}

impl AuditStore {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        for sub in ["tmp", "pets", "artifacts", "runs", "logs"] {
            fs::create_dir_all(data_dir.join(sub))
                .map_err(|e| format!("create {sub} dir: {e}"))?;
        }
        let log_path = data_dir.join("logs").join("server.log");
        let server_log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| format!("open server.log: {e}"))?;
        Ok(Self {
            data_dir,
            server_log: Mutex::new(server_log),
        })
    }

    pub fn run_dir(&self, task_id: &str) -> PathBuf {
        self.data_dir.join("runs").join(task_id)
    }

    pub fn prepare_run(
        &self,
        task_id: &str,
        request: &PetCreationRequest,
        client: &ClientInfo,
    ) -> Result<(PathBuf, PathBuf, PathBuf, RunRecord), String> {
        let run_dir = self.run_dir(task_id);
        let work_dir = run_dir.join("work");
        let pets_dir = run_dir.join("pets");
        fs::create_dir_all(&work_dir).map_err(|e| format!("create work dir: {e}"))?;
        fs::create_dir_all(&pets_dir).map_err(|e| format!("create pets dir: {e}"))?;

        let mut reference_path = None;
        if let Some(b64) = request.reference_image.as_ref().filter(|s| !s.is_empty()) {
            match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64) {
                Ok(bytes) => {
                    let path = run_dir.join("reference.png");
                    if let Err(e) = fs::write(&path, &bytes) {
                        self.server_log_line(
                            "warn",
                            &format!("[{task_id}] save reference failed: {e}"),
                        );
                    } else {
                        reference_path = Some(path.display().to_string());
                    }
                }
                Err(e) => {
                    self.server_log_line(
                        "warn",
                        &format!("[{task_id}] decode reference failed: {e}"),
                    );
                }
            }
        }

        let summary = RunRequestSummary {
            pet_name: request.pet_name.clone(),
            description: request.description.clone(),
            style_preset: request.style_preset.clone(),
            has_reference_image: reference_path.is_some()
                || request
                    .reference_image
                    .as_ref()
                    .map(|s| !s.is_empty())
                    .unwrap_or(false),
        };
        let request_path = run_dir.join("request.json");
        write_json(&request_path, &summary)?;

        let record = RunRecord {
            task_id: task_id.to_string(),
            status: "running".into(),
            client: client.clone(),
            request: summary,
            started_at: Utc::now(),
            finished_at: None,
            pet_id: None,
            artifact_sha256: None,
            error: None,
            run_dir: run_dir.display().to_string(),
            work_dir: Some(work_dir.display().to_string()),
            pet_dir: None,
            artifact_path: None,
            reference_path,
        };
        self.write_meta(&run_dir, &record)?;
        self.append_task_log(
            task_id,
            "info",
            &format!(
                "task created ip={} ua={} pet={} style={}",
                client.ip,
                client.user_agent.as_deref().unwrap_or("-"),
                request.pet_name,
                request.style_preset
            ),
        );
        self.server_log_line(
            "info",
            &format!(
                "[{task_id}] NEW task pet=\"{}\" ip={} style={}",
                request.pet_name, client.ip, request.style_preset
            ),
        );
        Ok((run_dir, work_dir, pets_dir, record))
    }

    pub fn write_meta(&self, run_dir: &Path, record: &RunRecord) -> Result<(), String> {
        write_json(&run_dir.join("meta.json"), record)
    }

    pub fn append_task_log(&self, task_id: &str, level: &str, message: &str) {
        let path = self.run_dir(task_id).join("task.log");
        let line = format!(
            "{} [{level}] {message}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
        self.server_log_line(level, &format!("[{task_id}] {message}"));
    }

    pub fn server_log_line(&self, level: &str, message: &str) {
        let line = format!(
            "{} [{level}] {message}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
        );
        if let Ok(mut f) = self.server_log.lock() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
        match level {
            "error" => tracing::error!("{message}"),
            "warn" => tracing::warn!("{message}"),
            "debug" => tracing::debug!("{message}"),
            _ => tracing::info!("{message}"),
        }
    }

    pub fn finalize_run(
        &self,
        mut record: RunRecord,
        status: &str,
        pet_id: Option<String>,
        artifact_sha256: Option<String>,
        error: Option<String>,
        pet_dir: Option<PathBuf>,
        artifact_path: Option<PathBuf>,
    ) -> Result<(), String> {
        record.status = status.to_string();
        record.finished_at = Some(Utc::now());
        record.pet_id = pet_id;
        record.artifact_sha256 = artifact_sha256;
        record.error = error;
        record.pet_dir = pet_dir.map(|p| p.display().to_string());
        record.artifact_path = artifact_path.map(|p| p.display().to_string());

        let run_dir = self.run_dir(&record.task_id);
        self.write_meta(&run_dir, &record)?;

        // Also mirror finished artifact into top-level artifacts/ for stable download path.
        if let Some(src) = record.artifact_path.as_ref() {
            let src_path = PathBuf::from(src);
            if src_path.exists() {
                let dest = self
                    .data_dir
                    .join("artifacts")
                    .join(format!("{}.zip", record.task_id));
                let _ = fs::copy(&src_path, &dest);
            }
        }

        // Mirror pet package into data/pets for easy browsing.
        if let (Some(pet_id), Some(src)) = (record.pet_id.as_ref(), record.pet_dir.as_ref()) {
            let src_path = PathBuf::from(src);
            let dest = self.data_dir.join("pets").join(pet_id);
            if src_path.exists() {
                let _ = copy_dir_recursive(&src_path, &dest);
            }
        }

        let history_path = self.data_dir.join("history.jsonl");
        let mut line = serde_json::to_string(&record).map_err(|e| format!("serialize history: {e}"))?;
        line.push('\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(history_path)
            .map_err(|e| format!("open history.jsonl: {e}"))?;
        f.write_all(line.as_bytes())
            .map_err(|e| format!("write history.jsonl: {e}"))?;

        self.append_task_log(
            &record.task_id,
            if status == "succeeded" {
                "info"
            } else if status == "cancelled" {
                "warn"
            } else {
                "error"
            },
            &format!(
                "FINAL status={status} pet_id={} error={}",
                record.pet_id.as_deref().unwrap_or("-"),
                record.error.as_deref().unwrap_or("-")
            ),
        );
        Ok(())
    }

    pub fn list_history(&self, limit: usize) -> Result<Vec<RunRecord>, String> {
        let path = self.data_dir.join("history.jsonl");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(path).map_err(|e| format!("read history: {e}"))?;
        let mut items = Vec::new();
        for line in text.lines().rev() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(item) = serde_json::from_str::<RunRecord>(line) {
                items.push(item);
                if items.len() >= limit {
                    break;
                }
            }
        }
        Ok(items)
    }

    pub fn read_run_meta(&self, task_id: &str) -> Option<RunRecord> {
        let path = self.run_dir(task_id).join("meta.json");
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }
    let data = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {e}"))?;
    fs::write(path, data).map_err(|e| format!("write {}: {e}", path.display()))
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_dir_all(dest).map_err(|e| format!("remove {}: {e}", dest.display()))?;
    }
    fs::create_dir_all(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("copy: {e}"))?;
        }
    }
    Ok(())
}
