use anyhow::{Context, Result};
use gstreamer::prelude::*;

use super::types::AudioDevice;

/// A camera device discovered via GStreamer DeviceMonitor.
#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
    /// Stable unique identifier from GStreamer device properties. Used for
    /// persistence so the correct camera is selected across runs even if
    /// enumeration order changes.
    pub uid: String,
    /// Native resolution (width, height) from device caps. Falls back to (1920, 1080).
    pub resolution: (u32, u32),
}

/// Resolve a camera UID to its current device index.
///
/// Falls back to matching by name, then to `fallback_index` if neither matches.
pub fn resolve_camera_index(cameras: &[CameraDevice], uid: &str, name: &str, fallback_index: u32) -> u32 {
    if !uid.is_empty() {
        if let Some(cam) = cameras.iter().find(|c| c.uid == uid) {
            return cam.device_index;
        }
    }
    if !name.is_empty() {
        if let Some(cam) = cameras.iter().find(|c| c.name == name) {
            return cam.device_index;
        }
    }
    fallback_index
}

/// A window available for capture, discovered via ScreenCaptureKit (macOS)
/// or platform-specific APIs.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
    pub bundle_id: String,
    /// Native window handle as u64. On Windows this is the HWND; on macOS the CGWindowID.
    pub native_handle: u64,
    /// Process/executable name (e.g., "notepad.exe"). Populated on Windows.
    pub process_name: String,
    /// Process ID of the owning process.
    pub process_id: u32,
    /// Bounding rect: (x, y, width, height) in logical points.
    #[allow(dead_code)]
    pub bounds: (f64, f64, f64, f64),
    pub is_on_screen: bool,
    pub is_fullscreen: bool,
}

/// An application with one or more capturable windows.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
    pub windows: Vec<WindowInfo>,
}

/// Info about a display available for capture, including its native resolution
/// in logical points.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub index: usize,
    pub width: u32,
    pub height: u32,
}

/// Enumerate available displays with their resolutions.
///
/// On Windows, uses the GStreamer device monitor to find display/screen devices.
/// Falls back to a single 1920x1080 default if enumeration fails.
#[cfg(target_os = "windows")]
pub fn enumerate_displays() -> Vec<DisplayInfo> {
    // Use Win32 EnumDisplayMonitors for accurate display info.
    use std::mem;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct MONITORINFOEXW {
        cbSize: u32,
        rcMonitor: RECT,
        rcWork: RECT,
        dwFlags: u32,
        szDevice: [u16; 32],
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    #[derive(Default)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    type HMONITOR = *mut std::ffi::c_void;
    type HDC = *mut std::ffi::c_void;
    type LPARAM = isize;

    unsafe extern "system" {
        fn EnumDisplayMonitors(
            hdc: HDC,
            lprc_clip: *const RECT,
            lpfn_enum: unsafe extern "system" fn(HMONITOR, HDC, *mut RECT, LPARAM) -> i32,
            dw_data: LPARAM,
        ) -> i32;
        fn GetMonitorInfoW(hmonitor: HMONITOR, lpmi: *mut MONITORINFOEXW) -> i32;
    }

    unsafe extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _lprc: *mut RECT,
        data: LPARAM,
    ) -> i32 {
        unsafe {
            let displays = &mut *(data as *mut Vec<DisplayInfo>);
            let mut info: MONITORINFOEXW = mem::zeroed();
            info.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(hmonitor, &mut info) != 0 {
                let width = (info.rcMonitor.right - info.rcMonitor.left) as u32;
                let height = (info.rcMonitor.bottom - info.rcMonitor.top) as u32;
                displays.push(DisplayInfo {
                    index: displays.len(),
                    width,
                    height,
                });
            }
        }
        1 // continue enumeration
    }

    let mut displays: Vec<DisplayInfo> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            monitor_enum_proc,
            &mut displays as *mut Vec<DisplayInfo> as LPARAM,
        );
    }

    if displays.is_empty() {
        log::warn!("No displays found via EnumDisplayMonitors, using 1920x1080 default");
        displays.push(DisplayInfo {
            index: 0,
            width: 1920,
            height: 1080,
        });
    }

    displays
}

/// Minimum window dimension (width or height) to filter out tiny/invisible windows.
#[cfg(target_os = "macos")]
const MIN_WINDOW_DIMENSION: f64 = 50.0;

/// Enumerate on-screen windows available for capture (macOS).
///
/// Uses ScreenCaptureKit to discover visible windows. Filters out windows with
/// empty titles, windows owned by Lodestone itself, and windows that are too
/// small to be useful capture targets.
#[cfg(target_os = "macos")]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let content = match super::screencapturekit::get_shareable_content() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("enumerate_windows: failed to get shareable content: {e}");
            return Vec::new();
        }
    };

    let own_pid = std::process::id() as i32;

    // Collect display bounds for fullscreen detection.
    let displays = unsafe { content.displays() };
    let display_count = displays.count();
    let mut display_bounds: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(display_count);
    for i in 0..display_count {
        let d = unsafe { displays.objectAtIndex_unchecked(i) };
        let frame = unsafe { d.frame() };
        display_bounds.push((
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        ));
    }

    let windows = unsafe { content.windows() };
    let count = windows.count();
    let mut results = Vec::new();

    for i in 0..count {
        let window = unsafe { windows.objectAtIndex_unchecked(i) };

        // Skip our own process's windows.
        let app = unsafe { window.owningApplication() };
        if let Some(ref a) = app
            && unsafe { a.processID() } == own_pid
        {
            continue;
        }

        // Extract window ID.
        let window_id = unsafe { window.windowID() };
        if window_id == 0 {
            continue;
        }

        // Extract title — skip windows with empty or missing titles.
        let title = match unsafe { window.title() } {
            Some(t) => {
                let s = t.to_string();
                if s.is_empty() {
                    continue;
                }
                s
            }
            None => continue,
        };

        // Extract owner info.
        let (owner_name, bundle_id) = if let Some(ref a) = app {
            let name = unsafe { a.applicationName() }.to_string();
            let bundle = unsafe { a.bundleIdentifier() }.to_string();
            (name, bundle)
        } else {
            (String::new(), String::new())
        };

        // Extract bounds and filter tiny windows.
        let frame = unsafe { window.frame() };
        let (x, y, w, h) = (
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        );
        if w < MIN_WINDOW_DIMENSION || h < MIN_WINDOW_DIMENSION {
            continue;
        }
        let bounds = (x, y, w, h);

        let is_on_screen = unsafe { window.isOnScreen() };
        let is_fullscreen = is_window_fullscreen(bounds, &display_bounds);

        results.push(WindowInfo {
            window_id,
            title,
            owner_name,
            bundle_id,
            native_handle: window_id as u64,
            process_name: String::new(),
            process_id: 0,
            bounds,
            is_on_screen,
            is_fullscreen,
        });
    }

    results
}

/// Win32 FFI declarations shared by window enumeration and capture helpers.
#[cfg(target_os = "windows")]
#[allow(unused, non_snake_case)]
mod win32 {
    use std::ffi::c_void;

    pub type HWND = *mut c_void;
    pub type LPARAM = isize;
    pub type BOOL = i32;

    #[repr(C)]
    #[derive(Default)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    pub type HANDLE = *mut c_void;
    pub type DWORD = u32;

    unsafe extern "system" {
        pub fn EnumWindows(
            lpfn_enum: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
            lp_data: LPARAM,
        ) -> BOOL;
        pub fn IsWindowVisible(hwnd: HWND) -> BOOL;
        pub fn GetWindowTextW(hwnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
        pub fn GetWindowTextLengthW(hwnd: HWND) -> i32;
        pub fn GetClassNameW(hwnd: HWND, lpClassName: *mut u16, nMaxCount: i32) -> i32;
        pub fn GetWindowThreadProcessId(hwnd: HWND, lpdwProcessId: *mut u32) -> u32;
        pub fn GetForegroundWindow() -> HWND;
        pub fn GetClientRect(hwnd: HWND, lpRect: *mut RECT) -> i32;
        pub fn GetWindowRect(hwnd: HWND, lpRect: *mut RECT) -> i32;
        pub fn IsWindow(hwnd: HWND) -> BOOL;
        pub fn OpenProcess(dwDesiredAccess: DWORD, bInheritHandle: BOOL, dwProcessId: DWORD) -> HANDLE;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn GetSystemMetrics(nIndex: i32) -> i32;
    }

    // QueryFullProcessImageNameW is in kernel32
    unsafe extern "system" {
        pub fn QueryFullProcessImageNameW(
            hProcess: HANDLE,
            dwFlags: DWORD,
            lpExeName: *mut u16,
            lpdwSize: *mut DWORD,
        ) -> BOOL;
    }

    pub const PROCESS_QUERY_LIMITED_INFORMATION: DWORD = 0x1000;
    pub const SM_CXSCREEN: i32 = 0;
    pub const SM_CYSCREEN: i32 = 1;
}

#[cfg(target_os = "windows")]
fn is_known_non_capturable_window(title: &str, class_name: &str) -> bool {
    matches!(title, "Program Manager")
        || matches!(class_name, "Progman" | "WorkerW" | "Shell_TrayWnd")
}

/// Get the process executable name for a PID on Windows.
#[cfg(target_os = "windows")]
fn get_process_name(pid: u32) -> String {
    let handle = unsafe {
        win32::OpenProcess(win32::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
    };
    if handle.is_null() {
        return String::new();
    }
    let mut buf = [0u16; 260];
    let mut size = buf.len() as u32;
    let ok = unsafe {
        win32::QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size)
    };
    unsafe { win32::CloseHandle(handle) };
    if ok == 0 || size == 0 {
        return String::new();
    }
    let path = String::from_utf16_lossy(&buf[..size as usize]);
    // Extract just the filename from the full path.
    path.rsplit('\\')
        .next()
        .unwrap_or(&path)
        .to_string()
}

/// Check if a window covers the full primary screen on Windows.
#[cfg(target_os = "windows")]
fn is_window_fullscreen_win(hwnd: win32::HWND) -> bool {
    let mut rect = win32::RECT::default();
    if unsafe { win32::GetWindowRect(hwnd, &mut rect) } == 0 {
        return false;
    }
    let screen_w = unsafe { win32::GetSystemMetrics(win32::SM_CXSCREEN) };
    let screen_h = unsafe { win32::GetSystemMetrics(win32::SM_CYSCREEN) };
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    // Allow 1px tolerance for borderless fullscreen windows.
    w >= screen_w - 1 && h >= screen_h - 1
}

/// Enumerate visible windows on Windows using EnumWindows.
/// Populates process names, window dimensions, and fullscreen detection.
#[cfg(target_os = "windows")]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    use win32::*;

    unsafe extern "system" fn enum_proc(hwnd: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let results = &mut *(data as *mut Vec<WindowInfo>);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            let title_len = GetWindowTextLengthW(hwnd);
            if title_len == 0 {
                return 1;
            }
            let mut title_buf = vec![0u16; (title_len + 1) as usize];
            GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
            let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);
            if title.is_empty() {
                return 1;
            }
            let class_name = get_window_class_from_hwnd(hwnd as u64);
            if is_known_non_capturable_window(&title, &class_name) {
                return 1;
            }

            // Get PID and skip our own process.
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == std::process::id() {
                return 1;
            }

            let (width, height) = get_window_size_from_hwnd(hwnd as u64).unwrap_or((0, 0));
            if width < 50 || height < 50 {
                return 1;
            }

            let process_name = get_process_name(pid);
            let is_fullscreen = is_window_fullscreen_win(hwnd);

            results.push(WindowInfo {
                window_id: hwnd as u32,
                title,
                owner_name: process_name.clone(),
                bundle_id: String::new(),
                native_handle: hwnd as u64,
                process_name,
                process_id: pid,
                bounds: (0.0, 0.0, width as f64, height as f64),
                is_on_screen: true,
                is_fullscreen,
            });
        }
        1
    }

    let mut results: Vec<WindowInfo> = Vec::new();
    unsafe {
        EnumWindows(enum_proc, &mut results as *mut Vec<WindowInfo> as LPARAM);
    }
    results
}

/// Fallback for non-macOS, non-Windows platforms.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[allow(dead_code)]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    Vec::new()
}

/// Get the foreground window handle (HWND) on Windows.
#[cfg(target_os = "windows")]
pub fn get_foreground_window_hwnd() -> Option<u64> {
    let hwnd = unsafe { win32::GetForegroundWindow() };
    if hwnd.is_null() {
        None
    } else if !is_window_capturable(hwnd as u64) {
        None
    } else {
        Some(hwnd as u64)
    }
}

/// Get the title of a window by its HWND on Windows.
#[cfg(target_os = "windows")]
pub fn get_window_title_from_hwnd(hwnd: u64) -> String {
    let hwnd = hwnd as win32::HWND;
    let len = unsafe { win32::GetWindowTextLengthW(hwnd) };
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    unsafe { win32::GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..len as usize])
}

/// Get the class name of a window by its HWND on Windows.
#[cfg(target_os = "windows")]
pub fn get_window_class_from_hwnd(hwnd: u64) -> String {
    let hwnd = hwnd as win32::HWND;
    let mut buf = vec![0u16; 256];
    let len = unsafe { win32::GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

/// Get the size (width, height) of a window by its HWND on Windows.
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn get_window_size_from_hwnd(hwnd: u64) -> Option<(u32, u32)> {
    let mut rect = win32::RECT::default();
    let ok = unsafe { win32::GetClientRect(hwnd as win32::HWND, &mut rect) };
    if ok == 0 {
        return None;
    }
    let w = (rect.right - rect.left) as u32;
    let h = (rect.bottom - rect.top) as u32;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}

/// Find a window matching process name and title substring on Windows.
/// Returns (hwnd, width, height) if found.
#[cfg(target_os = "windows")]
pub fn find_window_by_process_and_title(
    process_name: &str,
    window_title: &str,
) -> Option<(u64, u32, u32)> {
    let windows = enumerate_windows();
    // Prefer exact title match, then substring match.
    let exact = windows.iter().find(|w| {
        w.process_name.eq_ignore_ascii_case(process_name) && w.title == window_title
    });
    if let Some(w) = exact {
        return Some((w.native_handle, w.bounds.2 as u32, w.bounds.3 as u32));
    }
    // Substring match on title.
    let substr = windows.iter().find(|w| {
        w.process_name.eq_ignore_ascii_case(process_name)
            && (window_title.is_empty() || w.title.contains(window_title))
    });
    substr.map(|w| (w.native_handle, w.bounds.2 as u32, w.bounds.3 as u32))
}

/// Find the frontmost visible window for a process on Windows.
/// Returns (hwnd, width, height) if found.
#[cfg(target_os = "windows")]
pub fn find_app_window_by_process(process_name: &str) -> Option<(u64, u32, u32)> {
    let windows = enumerate_windows();
    // The first visible window for this process is typically the topmost.
    windows
        .iter()
        .find(|w| w.process_name.eq_ignore_ascii_case(process_name))
        .map(|w| (w.native_handle, w.bounds.2 as u32, w.bounds.3 as u32))
}

/// Find any fullscreen window on Windows.
/// Returns candidate windows in priority order.
#[cfg(target_os = "windows")]
pub fn find_fullscreen_windows() -> Vec<(u64, u32, u32)> {
    let windows = enumerate_windows();
    let foreground = get_foreground_window_hwnd();
    let mut fullscreen: Vec<(u64, u32, u32)> = windows
        .iter()
        .filter(|w| w.is_fullscreen)
        .map(|w| (w.native_handle, w.bounds.2 as u32, w.bounds.3 as u32))
        .collect();

    if let Some(fg) = foreground
        && let Some(idx) = fullscreen.iter().position(|(hwnd, _, _)| *hwnd == fg)
    {
        fullscreen.swap(0, idx);
    }

    fullscreen
}

/// Find any fullscreen window on Windows.
/// Returns (hwnd, width, height) if found.
#[cfg(target_os = "windows")]
pub fn find_fullscreen_window() -> Option<(u64, u32, u32)> {
    find_fullscreen_windows().into_iter().next()
}

/// Check if a window handle is still valid on Windows.
#[cfg(target_os = "windows")]
pub fn is_window_valid(hwnd: u64) -> bool {
    unsafe { win32::IsWindow(hwnd as win32::HWND) != 0 }
}

/// Returns `true` when the HWND looks like a real user window that WGC can target.
#[cfg(target_os = "windows")]
pub fn is_window_capturable(hwnd: u64) -> bool {
    if !is_window_valid(hwnd) {
        return false;
    }
    let title = get_window_title_from_hwnd(hwnd);
    let class_name = get_window_class_from_hwnd(hwnd);
    !title.is_empty()
        && !is_known_non_capturable_window(&title, &class_name)
        && get_window_size_from_hwnd(hwnd).is_some_and(|(w, h)| w >= 50 && h >= 50)
}

/// Group windows by owning application (macOS).
///
/// Returns a list of [`AppInfo`] sorted alphabetically by application name.
#[cfg(target_os = "macos")]
pub fn enumerate_applications() -> Vec<AppInfo> {
    let windows = enumerate_windows();
    let mut apps: std::collections::HashMap<String, AppInfo> = std::collections::HashMap::new();
    for win in windows {
        let entry = apps
            .entry(win.bundle_id.clone())
            .or_insert_with(|| AppInfo {
                bundle_id: win.bundle_id.clone(),
                name: win.owner_name.clone(),
                windows: Vec::new(),
            });
        entry.windows.push(win);
    }
    let mut result: Vec<AppInfo> = apps.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

/// Group windows by process name on Windows.
#[cfg(target_os = "windows")]
pub fn enumerate_applications() -> Vec<AppInfo> {
    let windows = enumerate_windows();
    let mut apps: std::collections::HashMap<String, AppInfo> = std::collections::HashMap::new();
    for win in windows {
        let key = win.process_name.clone();
        if key.is_empty() {
            continue;
        }
        let entry = apps.entry(key.clone()).or_insert_with(|| AppInfo {
            bundle_id: key,
            name: win.owner_name.clone(),
            windows: Vec::new(),
        });
        entry.windows.push(win);
    }
    let mut result: Vec<AppInfo> = apps.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

/// Fallback for non-macOS, non-Windows platforms.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn enumerate_applications() -> Vec<AppInfo> {
    Vec::new()
}

/// Returns `true` if the window bounds match any display bounds (within 1pt tolerance).
#[cfg(target_os = "macos")]
fn is_window_fullscreen(
    window_bounds: (f64, f64, f64, f64),
    displays: &[(f64, f64, f64, f64)],
) -> bool {
    let (wx, wy, ww, wh) = window_bounds;
    for &(dx, dy, dw, dh) in displays {
        if (wx - dx).abs() <= 1.0
            && (wy - dy).abs() <= 1.0
            && (ww - dw).abs() <= 1.0
            && (wh - dh).abs() <= 1.0
        {
            return true;
        }
    }
    false
}

/// Name prefixes that indicate a screen-capture source rather than a real camera.
#[cfg(target_os = "macos")]
const SCREEN_CAPTURE_HINTS: &[&str] = &["Capture screen"];
#[cfg(target_os = "windows")]
const SCREEN_CAPTURE_HINTS: &[&str] = &["Screen Capture", "screen-capture"];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const SCREEN_CAPTURE_HINTS: &[&str] = &["Capture screen", "Screen Capture"];

/// Known virtual audio device names used for system audio loopback.
#[cfg(target_os = "macos")]
const LOOPBACK_DEVICE_NAMES: &[&str] = &["BlackHole", "Soundflower", "Loopback"];
#[cfg(target_os = "windows")]
const LOOPBACK_DEVICE_NAMES: &[&str] = &[
    "CABLE Output",
    "VB-Audio",
    "Voicemeeter",
    "Stereo Mix",
    "What U Hear",
];
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const LOOPBACK_DEVICE_NAMES: &[&str] = &["Monitor of"];

/// Extract the highest resolution (by pixel count) from a GStreamer device's caps.
///
/// Handles fixed values. Returns `None` if no usable video caps found.
fn max_resolution_from_device(device: &gstreamer::Device) -> Option<(u32, u32)> {
    let caps = device.caps()?;
    let mut best: Option<(u32, u32)> = None;
    for s in caps.iter() {
        let w = match s.get::<i32>("width") {
            Ok(v) => v as u32,
            Err(_) => continue,
        };
        let h = match s.get::<i32>("height") {
            Ok(v) => v as u32,
            Err(_) => continue,
        };
        let pixels = w as u64 * h as u64;
        if best.is_none_or(|(bw, bh)| pixels > bw as u64 * bh as u64) {
            best = Some((w, h));
        }
    }
    best
}

/// Enumerate available camera devices using GStreamer's DeviceMonitor.
///
/// Filters out screen-capture sources by name heuristics. If no real cameras
/// remain, all `Video/Source` devices are returned so the user can choose.
/// Each device gets a stable UID from GStreamer properties for persistence.
pub fn enumerate_cameras() -> Result<Vec<CameraDevice>> {
    let monitor = gstreamer::DeviceMonitor::new();

    // Don't restrict caps to video/x-raw — some cameras only advertise
    // compressed formats (MJPEG, H.264) and would be hidden otherwise.
    monitor.add_filter(Some("Video/Source"), None);

    monitor.start().context("Failed to start device monitor")?;
    let devices = monitor.devices();
    monitor.stop();

    let all: Vec<CameraDevice> = devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let resolution = max_resolution_from_device(device).unwrap_or((1920, 1080));
            let name = device.display_name().to_string();
            let uid = device
                .properties()
                .and_then(|props| props.get::<String>("unique-id").ok())
                .unwrap_or_else(|| name.clone());
            CameraDevice {
                device_index: i as u32,
                name,
                uid,
                resolution,
            }
        })
        .collect();

    // Try to filter out screen-capture devices.
    let filtered: Vec<CameraDevice> = all
        .iter()
        .filter(|cam| {
            !SCREEN_CAPTURE_HINTS
                .iter()
                .any(|hint| cam.name.contains(hint))
        })
        .cloned()
        .collect();

    // If filtering removed everything, return the unfiltered list.
    if filtered.is_empty() {
        Ok(all)
    } else {
        Ok(filtered)
    }
}

/// Enumerate available audio input devices using GStreamer's DeviceMonitor.
pub fn enumerate_audio_input_devices() -> Result<Vec<AudioDevice>> {
    let monitor = gstreamer::DeviceMonitor::new();

    let caps = gstreamer::Caps::new_empty_simple("audio/x-raw");
    monitor.add_filter(Some("Audio/Source"), Some(&caps));

    monitor.start().context("Failed to start device monitor")?;
    let devices = monitor.devices();
    monitor.stop();

    let mut result = Vec::new();
    for device in devices {
        let name = device.display_name().to_string();
        let uid = device
            .properties()
            .and_then(|props| props.get::<String>("unique-id").ok())
            .unwrap_or_else(|| name.clone());

        let is_loopback = LOOPBACK_DEVICE_NAMES
            .iter()
            .any(|known| name.contains(known));

        result.push(AudioDevice {
            uid,
            name,
            is_loopback,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_does_not_panic() {
        gstreamer::init().unwrap();
        match enumerate_audio_input_devices() {
            Ok(devices) => {
                for d in &devices {
                    assert!(!d.name.is_empty());
                    assert!(!d.uid.is_empty());
                }
            }
            Err(e) => {
                eprintln!("Skipping device enumeration test: {e}");
            }
        }
    }

    #[test]
    fn enumerate_cameras_does_not_panic() {
        gstreamer::init().unwrap();
        match enumerate_cameras() {
            Ok(cameras) => {
                for cam in &cameras {
                    assert!(!cam.name.is_empty());
                }
            }
            Err(e) => {
                eprintln!("Skipping camera enumeration test: {e}");
            }
        }
    }

    #[test]
    fn enumerate_windows_does_not_panic() {
        let windows = enumerate_windows();
        for w in &windows {
            assert!(!w.title.is_empty());
            assert!(w.window_id != 0);
            assert!(w.bounds.2 >= 50.0);
            assert!(w.bounds.3 >= 50.0);
        }
    }

    #[test]
    fn loopback_detection() {
        let is_loopback = |name: &str| {
            LOOPBACK_DEVICE_NAMES
                .iter()
                .any(|known| name.contains(known))
        };

        #[cfg(target_os = "macos")]
        {
            assert!(is_loopback("BlackHole 2ch"));
            assert!(is_loopback("Soundflower (2ch)"));
        }
        #[cfg(target_os = "windows")]
        {
            assert!(is_loopback("CABLE Output (VB-Audio Virtual Cable)"));
            assert!(is_loopback("Stereo Mix (Realtek Audio)"));
        }
        assert!(!is_loopback("Built-in Microphone"));
        assert!(!is_loopback("USB Audio Device"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn known_shell_windows_are_not_capturable() {
        assert!(is_known_non_capturable_window("Program Manager", "Progman"));
        assert!(is_known_non_capturable_window("Anything", "WorkerW"));
        assert!(!is_known_non_capturable_window("Untitled - Notepad", "Notepad"));
    }
}
