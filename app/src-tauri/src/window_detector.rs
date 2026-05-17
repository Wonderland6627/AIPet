#[cfg(windows)]
use windows::{
    Win32::{
        Foundation::CloseHandle,
        Media::Audio::{
            eCapture, eRender, Endpoints::IAudioMeterInformation, DEVICE_STATE_ACTIVE,
            ERole, IMMDeviceEnumerator, MMDeviceEnumerator,
        },
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
            SystemInformation::GetTickCount,
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
};

#[cfg(windows)]
static COM_INIT: std::sync::Once = std::sync::Once::new();

#[cfg(windows)]
fn ensure_com() {
    COM_INIT.call_once(|| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    });
}

#[cfg(windows)]
fn endpoint_peak_active(data_flow: windows::Win32::Media::Audio::EDataFlow) -> bool {
    ensure_com();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(_) => return false,
            };
        let device = match enumerator.GetDefaultAudioEndpoint(data_flow, ERole(0)) {
            Ok(d) => d,
            Err(_) => return false,
        };
        let state = match device.GetState() {
            Ok(s) => s,
            Err(_) => return false,
        };
        if state != DEVICE_STATE_ACTIVE {
            return false;
        }
        let meter: IAudioMeterInformation = match device.Activate(CLSCTX_ALL, None) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let peak = match meter.GetPeakValue() {
            Ok(p) => p,
            Err(_) => return false,
        };
        peak > 0.02
    }
}

#[cfg(windows)]
pub fn get_visible_window_pids() -> std::collections::HashSet<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, IsWindowVisible, GW_OWNER,
    };
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::core::BOOL;
    use std::collections::HashSet;

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            let owner = GetWindow(hwnd, GW_OWNER);
            if !owner.is_err() && owner.unwrap_or(HWND::default()).0 != std::ptr::null_mut() {
                return BOOL(1);
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 {
                let set = &mut *(lparam.0 as *mut HashSet<u32>);
                set.insert(pid);
            }
            BOOL(1)
        }
    }

    let mut pids = HashSet::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut pids as *mut HashSet<u32> as isize),
        );
    }
    pids
}

#[cfg(not(windows))]
pub fn get_visible_window_pids() -> std::collections::HashSet<u32> {
    std::collections::HashSet::new()
}

#[cfg(windows)]
pub fn get_active_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);

        result.ok()?;

        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(|s| s.to_string())
    }
}

#[cfg(windows)]
pub fn get_idle_seconds() -> Option<u64> {
    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if !GetLastInputInfo(&mut info).as_bool() {
            return None;
        }
        let tick = GetTickCount();
        let idle_ms = tick.saturating_sub(info.dwTime);
        Some((idle_ms / 1000) as u64)
    }
}

#[cfg(windows)]
pub fn is_audio_playing() -> bool {
    endpoint_peak_active(eRender)
}

#[cfg(windows)]
pub fn is_microphone_active() -> bool {
    endpoint_peak_active(eCapture)
}

#[cfg(not(windows))]
pub fn get_active_process_name() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn get_idle_seconds() -> Option<u64> {
    None
}

#[cfg(not(windows))]
pub fn is_audio_playing() -> bool {
    false
}

#[cfg(not(windows))]
pub fn is_microphone_active() -> bool {
    false
}
