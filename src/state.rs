use crate::gstreamer::{GstCommand, GstError};
use crate::scene::{LibrarySource, Scene, SceneId, SourceId};
use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub enum StreamStatus {
    Offline,
    #[allow(dead_code)]
    Connecting,
    Live {
        uptime_secs: f64,
        bitrate_kbps: f64,
        dropped_frames: u64,
    },
}

impl StreamStatus {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }
}

/// Whether the app is currently recording.
#[derive(Debug, Clone)]
pub enum RecordingStatus {
    Idle,
    Recording { path: std::path::PathBuf },
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub sources: Vec<LibrarySource>,
    pub active_scene_id: Option<SceneId>,
    pub selected_source_id: Option<SourceId>,
    pub audio_levels: crate::gstreamer::AudioLevelUpdate,
    pub available_audio_devices: Vec<crate::gstreamer::AudioDevice>,
    pub available_cameras: Vec<crate::gstreamer::CameraDevice>,
    pub available_windows: Vec<crate::gstreamer::WindowInfo>,
    pub stream_status: StreamStatus,
    pub settings: AppSettings,
    pub settings_dirty: bool,
    pub settings_last_changed: std::time::Instant,
    pub scenes_dirty: bool,
    pub scenes_last_changed: std::time::Instant,
    pub next_scene_id: u64,
    pub next_source_id: u64,
    pub monitor_count: usize,
    pub capture_active: bool,
    pub active_errors: Vec<GstError>,
    pub recording_status: RecordingStatus,
    pub command_tx: Option<tokio::sync::mpsc::Sender<GstCommand>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scenes: Vec::new(),
            sources: Vec::new(),
            active_scene_id: None,
            selected_source_id: None,
            audio_levels: crate::gstreamer::AudioLevelUpdate::default(),
            available_audio_devices: Vec::new(),
            available_cameras: Vec::new(),
            available_windows: Vec::new(),
            stream_status: StreamStatus::Offline,
            settings: AppSettings::default(),
            settings_dirty: false,
            settings_last_changed: std::time::Instant::now(),
            scenes_dirty: false,
            scenes_last_changed: std::time::Instant::now(),
            next_scene_id: 1,
            next_source_id: 1,
            monitor_count: 1,
            capture_active: true,
            active_errors: Vec::new(),
            recording_status: RecordingStatus::Idle,
            command_tx: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_state() {
        let state = AppState::default();
        assert!(matches!(state.stream_status, StreamStatus::Offline));
        assert!(state.scenes.is_empty());
    }

    #[test]
    fn stream_status_is_live() {
        let status = StreamStatus::Live {
            uptime_secs: 120.0,
            bitrate_kbps: 4500.0,
            dropped_frames: 0,
        };
        assert!(status.is_live());
        assert!(!StreamStatus::Offline.is_live());
    }
}
