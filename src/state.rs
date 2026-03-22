use crate::obs::{Scene, SceneId, Source, SourceId};
use crate::settings::AppSettings;

#[derive(Debug, Clone)]
pub struct AudioLevel {
    pub source_id: SourceId,
    pub current_db: f32,
    #[allow(dead_code)]
    pub peak_db: f32,
}

impl AudioLevel {
    pub fn new(source_id: SourceId, current_db: f32, peak_db: f32) -> Self {
        Self {
            source_id,
            current_db: current_db.clamp(-60.0, 0.0),
            peak_db: peak_db.clamp(-60.0, 0.0),
        }
    }
}

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

#[derive(Debug, Clone)]
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub sources: Vec<Source>,
    pub active_scene_id: Option<SceneId>,
    pub audio_levels: Vec<AudioLevel>,
    pub stream_status: StreamStatus,
    pub settings: AppSettings,
    pub settings_dirty: bool,
    pub settings_last_changed: std::time::Instant,
    pub preview_width: u32,
    pub preview_height: u32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scenes: Vec::new(),
            sources: Vec::new(),
            active_scene_id: None,
            audio_levels: Vec::new(),
            stream_status: StreamStatus::Offline,
            settings: AppSettings::default(),
            settings_dirty: false,
            settings_last_changed: std::time::Instant::now(),
            preview_width: 0,
            preview_height: 0,
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

    #[test]
    fn audio_level_clamping() {
        let level = AudioLevel::new(SourceId(1), -80.0, -80.0);
        assert!(level.current_db >= -60.0);
        assert_eq!(level.source_id, SourceId(1));
    }
}
