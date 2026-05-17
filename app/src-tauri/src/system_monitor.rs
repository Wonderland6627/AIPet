use crate::window_detector;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProcessItem {
    pub exe: String,
    pub display_name: String,
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

fn normalize_process_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let base = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed);
    let lower = base.to_lowercase();
    if lower.ends_with(".exe") {
        return Some(lower);
    }
    Some(format!("{lower}.exe"))
}

fn display_name_without_exe(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.len() <= 4 {
        return trimmed.to_string();
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.ends_with(".exe") {
        return trimmed.to_string();
    }
    trimmed[..trimmed.len() - 4].to_string()
}

#[cfg(windows)]
fn process_file_description(path: &std::path::Path) -> Option<String> {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
        },
    };

    let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    path_wide.push(0);
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(path_wide.as_ptr()), None) };
    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];
    let ok = unsafe {
        GetFileVersionInfoW(
            PCWSTR(path_wide.as_ptr()),
            Some(0),
            size,
            buffer.as_mut_ptr() as *mut c_void,
        )
    };
    if ok.is_err() {
        return None;
    }

    let query_value = |query: &str| -> Option<String> {
        let mut query_wide: Vec<u16> = query.encode_utf16().collect();
        query_wide.push(0);
        let mut out_ptr: *mut c_void = std::ptr::null_mut();
        let mut out_len: u32 = 0;
        let found = unsafe {
            VerQueryValueW(
                buffer.as_ptr() as *const c_void,
                PCWSTR(query_wide.as_ptr()),
                &mut out_ptr,
                &mut out_len,
            )
        }
        .as_bool();
        if !found || out_ptr.is_null() || out_len == 0 {
            return None;
        }
        let text_slice =
            unsafe { std::slice::from_raw_parts(out_ptr as *const u16, out_len as usize) };
        let text = String::from_utf16_lossy(text_slice)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        Some(text)
    };

    let mut trans_ptr: *mut c_void = std::ptr::null_mut();
    let mut trans_len: u32 = 0;
    let mut trans_query = "\\VarFileInfo\\Translation".encode_utf16().collect::<Vec<_>>();
    trans_query.push(0);
    let trans_ok = unsafe {
        VerQueryValueW(
            buffer.as_ptr() as *const c_void,
            PCWSTR(trans_query.as_ptr()),
            &mut trans_ptr,
            &mut trans_len,
        )
    }
    .as_bool();
    if trans_ok && !trans_ptr.is_null() && trans_len >= 4 {
        let translation =
            unsafe { std::slice::from_raw_parts(trans_ptr as *const u16, (trans_len / 2) as usize) };
        if translation.len() >= 2 {
            let query = format!(
                "\\StringFileInfo\\{:04x}{:04x}\\FileDescription",
                translation[0], translation[1]
            );
            if let Some(found) = query_value(&query) {
                return Some(found);
            }
        }
    }

    query_value("\\StringFileInfo\\040904b0\\FileDescription")
}

#[cfg(not(windows))]
fn process_file_description(_path: &std::path::Path) -> Option<String> {
    None
}

fn resolve_process_display_name(process: &sysinfo::Process, normalized_exe: &str) -> String {
    if let Some(path) = process.exe() {
        if let Some(desc) = process_file_description(path) {
            return desc;
        }
    }
    let raw = process.name().to_string_lossy();
    let raw_display = display_name_without_exe(&raw);
    if !raw_display.is_empty() {
        return raw_display;
    }
    let exe_display = display_name_without_exe(normalized_exe);
    if !exe_display.is_empty() {
        return exe_display;
    }
    normalized_exe.to_string()
}

#[tauri::command]
pub fn list_running_processes() -> Vec<RunningProcessItem> {
    let pids = crate::window_detector::get_visible_window_pids();
    if pids.is_empty() {
        return Vec::new();
    }

    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut processes = HashMap::<String, String>::new();
    for process in sys.processes().values() {
        let pid = process.pid().as_u32();
        if !pids.contains(&pid) {
            continue;
        }
        let raw = process.name().to_string_lossy();
        let Some(exe) = normalize_process_name(&raw) else {
            continue;
        };
        processes
            .entry(exe.clone())
            .or_insert_with(|| resolve_process_display_name(process, &exe));
    }

    let mut list: Vec<RunningProcessItem> = processes
        .into_iter()
        .map(|(exe, display_name)| RunningProcessItem { exe, display_name })
        .collect();
    list.sort_by_key(|x| (x.display_name.to_lowercase(), x.exe.clone()));
    list
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
