use crate::window_detector;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const ACTIVE_IDLE_THRESHOLD_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemState {
    pub active_process: String,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub audio_playing: bool,
    pub microphone_active: bool,
    pub idle_seconds: u64,
    pub focus_seconds: u64,
}

struct FocusTracker {
    active_since: Option<Instant>,
    last_idle_secs: u64,
}

impl FocusTracker {
    fn new() -> Self {
        Self {
            active_since: None,
            last_idle_secs: 0,
        }
    }

    fn update(&mut self, idle_seconds: u64) -> u64 {
        if idle_seconds >= ACTIVE_IDLE_THRESHOLD_SECS {
            self.active_since = None;
            self.last_idle_secs = idle_seconds;
            return 0;
        }

        if self.active_since.is_none() {
            self.active_since = Some(Instant::now());
        }

        self.active_since
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }
}

pub fn spawn_system_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut sys = sysinfo::System::new();
        let mut focus = FocusTracker::new();
        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            let cpu_percent = sys.global_cpu_usage();
            let total_mem = sys.total_memory();
            let used_mem = sys.used_memory();
            let memory_percent = if total_mem == 0 {
                0.0
            } else {
                (used_mem as f64 / total_mem as f64 * 100.0) as f32
            };

            let active_process = window_detector::get_active_process_name().unwrap_or_default();
            let idle_seconds = window_detector::get_idle_seconds().unwrap_or(0);
            let focus_seconds = focus.update(idle_seconds);
            let audio_playing = window_detector::is_audio_playing();
            let microphone_active = window_detector::is_microphone_active();

            let state = SystemState {
                active_process: active_process.clone(),
                cpu_percent,
                memory_percent,
                audio_playing,
                microphone_active,
                idle_seconds,
                focus_seconds,
            };

            let _ = app.emit("system-state-changed", &state);

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}
