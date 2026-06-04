// ──────────────────────────── Windows ────────────────────────────

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

// ──────────────────────────── macOS ────────────────────────────

#[cfg(target_os = "macos")]
mod macos_ffi {
    use std::ffi::{c_char, c_void};

    pub type CFTypeRef = *const c_void;

    pub const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    pub const CF_NUMBER_SINT32_TYPE: i64 = 3;
    pub const CG_WINDOW_LIST_ON_SCREEN_ONLY: u32 = 1;
    pub const CG_WINDOW_LIST_EXCLUDE_DESKTOP: u32 = 1 << 4;
    pub const CG_NULL_WINDOW_ID: u32 = 0;
    pub const CG_EVENT_SOURCE_COMBINED_SESSION: i32 = 0;
    pub const CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
        pub fn CGEventSourceSecondsSinceLastEventType(
            state_id: i32,
            event_type: u32,
        ) -> f64;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        pub fn CFRelease(cf: CFTypeRef);
        pub fn CFArrayGetCount(array: CFTypeRef) -> isize;
        pub fn CFArrayGetValueAtIndex(array: CFTypeRef, idx: isize) -> CFTypeRef;
        pub fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        pub fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFTypeRef;
        pub fn CFStringGetLength(string: CFTypeRef) -> isize;
        pub fn CFStringGetCString(
            string: CFTypeRef,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
        pub fn CFNumberGetValue(
            number: CFTypeRef,
            the_type: i64,
            value_ptr: *mut c_void,
        ) -> u8;
    }
}

#[cfg(target_os = "macos")]
fn cf_string_create(s: &str) -> macos_ffi::CFTypeRef {
    let Ok(c) = std::ffi::CString::new(s) else {
        return std::ptr::null();
    };
    unsafe {
        macos_ffi::CFStringCreateWithCString(
            std::ptr::null(),
            c.as_ptr(),
            macos_ffi::CF_STRING_ENCODING_UTF8,
        )
    }
}

#[cfg(target_os = "macos")]
fn cf_string_to_rust(cf: macos_ffi::CFTypeRef) -> Option<String> {
    if cf.is_null() {
        return None;
    }
    unsafe {
        let len = macos_ffi::CFStringGetLength(cf);
        if len <= 0 {
            return None;
        }
        let buf_size = len * 4 + 1;
        let mut buf: Vec<std::ffi::c_char> = vec![0; buf_size as usize];
        if macos_ffi::CFStringGetCString(
            cf,
            buf.as_mut_ptr(),
            buf_size,
            macos_ffi::CF_STRING_ENCODING_UTF8,
        ) == 0
        {
            return None;
        }
        let c_str = std::ffi::CStr::from_ptr(buf.as_ptr());
        Some(c_str.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "macos")]
fn dict_get_string(dict: macos_ffi::CFTypeRef, key: &str) -> Option<String> {
    let cf_key = cf_string_create(key);
    if cf_key.is_null() {
        return None;
    }
    unsafe {
        let val = macos_ffi::CFDictionaryGetValue(dict, cf_key);
        macos_ffi::CFRelease(cf_key);
        cf_string_to_rust(val)
    }
}

#[cfg(target_os = "macos")]
fn dict_get_i32(dict: macos_ffi::CFTypeRef, key: &str) -> Option<i32> {
    let cf_key = cf_string_create(key);
    if cf_key.is_null() {
        return None;
    }
    unsafe {
        let val = macos_ffi::CFDictionaryGetValue(dict, cf_key);
        macos_ffi::CFRelease(cf_key);
        if val.is_null() {
            return None;
        }
        let mut out: i32 = 0;
        if macos_ffi::CFNumberGetValue(
            val,
            macos_ffi::CF_NUMBER_SINT32_TYPE,
            &mut out as *mut i32 as *mut std::ffi::c_void,
        ) != 0
        {
            Some(out)
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
pub fn get_visible_window_pids() -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    let mut pids = HashSet::new();
    unsafe {
        let list = macos_ffi::CGWindowListCopyWindowInfo(
            macos_ffi::CG_WINDOW_LIST_ON_SCREEN_ONLY
                | macos_ffi::CG_WINDOW_LIST_EXCLUDE_DESKTOP,
            macos_ffi::CG_NULL_WINDOW_ID,
        );
        if list.is_null() {
            return pids;
        }
        let count = macos_ffi::CFArrayGetCount(list);
        for i in 0..count {
            let dict = macos_ffi::CFArrayGetValueAtIndex(list, i);
            let layer = dict_get_i32(dict, "kCGWindowLayer").unwrap_or(-1);
            if layer != 0 {
                continue;
            }
            if let Some(pid) = dict_get_i32(dict, "kCGWindowOwnerPID") {
                if pid > 0 {
                    pids.insert(pid as u32);
                }
            }
        }
        macos_ffi::CFRelease(list);
    }
    pids
}

#[cfg(target_os = "macos")]
pub fn get_active_process_name() -> Option<String> {
    unsafe {
        let list = macos_ffi::CGWindowListCopyWindowInfo(
            macos_ffi::CG_WINDOW_LIST_ON_SCREEN_ONLY
                | macos_ffi::CG_WINDOW_LIST_EXCLUDE_DESKTOP,
            macos_ffi::CG_NULL_WINDOW_ID,
        );
        if list.is_null() {
            return None;
        }
        let count = macos_ffi::CFArrayGetCount(list);
        for i in 0..count {
            let dict = macos_ffi::CFArrayGetValueAtIndex(list, i);
            let layer = dict_get_i32(dict, "kCGWindowLayer").unwrap_or(-1);
            if layer != 0 {
                continue;
            }
            let name = dict_get_string(dict, "kCGWindowOwnerName");
            macos_ffi::CFRelease(list);
            return name;
        }
        macos_ffi::CFRelease(list);
        None
    }
}

#[cfg(target_os = "macos")]
pub fn get_idle_seconds() -> Option<u64> {
    unsafe {
        let seconds = macos_ffi::CGEventSourceSecondsSinceLastEventType(
            macos_ffi::CG_EVENT_SOURCE_COMBINED_SESSION,
            macos_ffi::CG_ANY_INPUT_EVENT_TYPE,
        );
        if seconds < 0.0 {
            return None;
        }
        Some(seconds as u64)
    }
}

#[cfg(target_os = "macos")]
pub fn is_audio_playing() -> bool {
    // TODO: implement via Core Audio peak metering
    false
}

#[cfg(target_os = "macos")]
pub fn is_microphone_active() -> bool {
    // TODO: implement via Core Audio capture metering
    false
}

// ──────────────────────── fallback (Linux etc.) ─────────────────────────

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get_visible_window_pids() -> std::collections::HashSet<u32> {
    std::collections::HashSet::new()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get_active_process_name() -> Option<String> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn get_idle_seconds() -> Option<u64> {
    None
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_audio_playing() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_microphone_active() -> bool {
    false
}
