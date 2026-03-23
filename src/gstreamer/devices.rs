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
