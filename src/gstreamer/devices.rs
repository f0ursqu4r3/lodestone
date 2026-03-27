use anyhow::{Context, Result};
use gstreamer::prelude::*;

use super::types::AudioDevice;

/// A camera device discovered via GStreamer DeviceMonitor.
#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
    /// Native resolution (width, height) from device caps. Falls back to (1920, 1080).
    pub resolution: (u32, u32),
}

/// A window available for capture, discovered via ScreenCaptureKit.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
    pub bundle_id: String,
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

/// Minimum window dimension (width or height) to filter out tiny/invisible windows.
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
            bounds,
            is_on_screen,
            is_fullscreen,
        });
    }

    results
}

/// Fallback for non-macOS platforms — window enumeration is not yet supported.
#[cfg(not(target_os = "macos"))]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    Vec::new()
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

/// Fallback for non-macOS platforms.
#[cfg(not(target_os = "macos"))]
pub fn enumerate_applications() -> Vec<AppInfo> {
    Vec::new()
}

/// Returns `true` if the window bounds match any display bounds (within 1pt tolerance).
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

/// Name substrings that indicate a screen-capture source rather than a real camera.
const SCREEN_CAPTURE_HINTS: &[&str] = &["Screen", "Capture screen", "Display"];

/// Known virtual audio device names used for system audio loopback.
const LOOPBACK_DEVICE_NAMES: &[&str] = &["BlackHole", "Soundflower", "Loopback"];

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
pub fn enumerate_cameras() -> Result<Vec<CameraDevice>> {
    let monitor = gstreamer::DeviceMonitor::new();

    let caps = gstreamer::Caps::new_empty_simple("video/x-raw");
    monitor.add_filter(Some("Video/Source"), Some(&caps));

    monitor.start().context("Failed to start device monitor")?;
    let devices = monitor.devices();
    monitor.stop();

    let all: Vec<CameraDevice> = devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let resolution = max_resolution_from_device(device).unwrap_or((1920, 1080));
            CameraDevice {
                device_index: i as u32,
                name: device.display_name().to_string(),
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
            assert!(w.bounds.2 >= super::MIN_WINDOW_DIMENSION);
            assert!(w.bounds.3 >= super::MIN_WINDOW_DIMENSION);
        }
    }

    #[test]
    fn loopback_detection() {
        let is_loopback = |name: &str| {
            LOOPBACK_DEVICE_NAMES
                .iter()
                .any(|known| name.contains(known))
        };

        assert!(is_loopback("BlackHole 2ch"));
        assert!(is_loopback("Soundflower (2ch)"));
        assert!(!is_loopback("Built-in Microphone"));
        assert!(!is_loopback("USB Audio Device"));
    }
}
