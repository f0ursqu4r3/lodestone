use std::collections::HashMap;
use std::path::PathBuf;

use crate::scene::SourceId;

/// Raw RGBA frame data from the capture pipeline.
#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Backend-authoritative runtime state for long-lived output pipelines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputRuntimeState {
    pub stream_active: bool,
    pub recording_path: Option<PathBuf>,
    pub virtual_camera_active: bool,
}

/// Pipeline statistics sent periodically from the GStreamer thread.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PipelineStats {
    pub bitrate_kbps: f64,
    pub dropped_frames: u64,
    pub total_frames: u64,
    pub uptime_secs: f64,
}

impl Default for PipelineStats {
    fn default() -> Self {
        Self {
            bitrate_kbps: 0.0,
            dropped_frames: 0,
            total_frames: 0,
            uptime_secs: 0.0,
        }
    }
}

/// Audio level data from the GStreamer `level` element.
#[derive(Debug, Clone, Default)]
pub struct AudioLevelUpdate {
    pub mic: Option<AudioLevels>,
    pub system: Option<AudioLevels>,
    pub source_levels: HashMap<SourceId, AudioLevels>,
}

/// Peak and RMS levels for a single audio source.
#[derive(Debug, Clone)]
pub struct AudioLevels {
    pub peak_db: f32,
    pub rms_db: f32,
}

/// An audio input device discovered by the DeviceMonitor.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub uid: String,
    pub name: String,
    pub is_loopback: bool,
}
