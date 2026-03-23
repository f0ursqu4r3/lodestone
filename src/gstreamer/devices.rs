use anyhow::{Context, Result};
use gstreamer::prelude::*;

use super::types::AudioDevice;

/// A camera device discovered via GStreamer DeviceMonitor.
#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
}

/// A window available for capture, discovered via CoreGraphics.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
}

/// Minimum window dimension (width or height) to filter out tiny/invisible windows.
const MIN_WINDOW_DIMENSION: f64 = 50.0;

/// Enumerate on-screen windows available for capture (macOS).
///
/// Uses CoreGraphics `CGWindowListCopyWindowInfo` to discover visible windows.
/// Filters out windows with empty titles, windows owned by Lodestone itself,
/// and windows that are too small to be useful capture targets.
#[cfg(target_os = "macos")]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly,
    };

    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;

    let window_list = match copy_window_info(options, kCGNullWindowID) {
        Some(list) => list,
        None => return Vec::new(),
    };

    let own_pid = std::process::id();

    let key_number = CFString::new("kCGWindowNumber");
    let key_owner = CFString::new("kCGWindowOwnerName");
    let key_name = CFString::new("kCGWindowName");
    let key_owner_pid = CFString::new("kCGWindowOwnerPID");
    let key_bounds = CFString::new("kCGWindowBounds");

    let mut results = Vec::new();

    for i in 0..window_list.len() {
        // Each entry is a CFDictionary; get it as a raw CFType and downcast.
        let entry: CFType = unsafe { CFType::wrap_under_get_rule(*window_list.get(i).unwrap()) };
        let dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(entry.as_CFTypeRef() as *const _) };

        // Extract owner PID and skip our own windows.
        let owner_pid = dict.find(&key_owner_pid).and_then(|v| unsafe {
            CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _).to_i64()
        });
        if let Some(pid) = owner_pid
            && pid as u32 == own_pid
        {
            continue;
        }

        // Extract window ID.
        let window_id = match dict.find(&key_number).and_then(|v| unsafe {
            CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _).to_i64()
        }) {
            Some(id) if id > 0 => id as u32,
            _ => continue,
        };

        // Extract title — skip windows with empty or missing titles.
        let title = match dict.find(&key_name) {
            Some(v) => {
                let s = unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _) };
                let t = s.to_string();
                if t.is_empty() {
                    continue;
                }
                t
            }
            None => continue,
        };

        // Extract owner name.
        let owner_name = dict
            .find(&key_owner)
            .map(|v| unsafe {
                CFString::wrap_under_get_rule(v.as_CFTypeRef() as *const _).to_string()
            })
            .unwrap_or_default();

        // Filter out very small windows by checking bounds dictionary.
        if let Some(bounds_val) = dict.find(&key_bounds) {
            let bounds_dict: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(bounds_val.as_CFTypeRef() as *const _) };
            let width_key = CFString::new("Width");
            let height_key = CFString::new("Height");

            let width = bounds_dict
                .find(&width_key)
                .and_then(|v| unsafe {
                    CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _).to_f64()
                })
                .unwrap_or(0.0);
            let height = bounds_dict
                .find(&height_key)
                .and_then(|v| unsafe {
                    CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as *const _).to_f64()
                })
                .unwrap_or(0.0);

            if width < MIN_WINDOW_DIMENSION || height < MIN_WINDOW_DIMENSION {
                continue;
            }
        }

        results.push(WindowInfo {
            window_id,
            title,
            owner_name,
        });
    }

    results
}

/// Fallback for non-macOS platforms — window enumeration is not yet supported.
#[cfg(not(target_os = "macos"))]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    Vec::new()
}

/// Name substrings that indicate a screen-capture source rather than a real camera.
const SCREEN_CAPTURE_HINTS: &[&str] = &["Screen", "Capture screen", "Display"];

/// Known virtual audio device names used for system audio loopback.
const LOOPBACK_DEVICE_NAMES: &[&str] = &["BlackHole", "Soundflower", "Loopback"];

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
        .map(|(i, device)| CameraDevice {
            device_index: i as u32,
            name: device.display_name().to_string(),
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
