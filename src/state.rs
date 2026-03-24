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
    pub library: Vec<LibrarySource>,
    pub active_scene_id: Option<SceneId>,
    /// Source selected in the scene (sources panel / preview). Drives transform handles.
    pub selected_source_id: Option<SourceId>,
    /// Source selected in the library panel. Drives properties panel in "Library Defaults" mode.
    pub selected_library_source_id: Option<SourceId>,
    /// Source to flash briefly in the scene (sources panel + preview) when selected in library.
    pub flash_source_id: Option<SourceId>,
    /// When the flash started, for animation timing.
    pub flash_start: Option<std::time::Instant>,
    /// Source currently being renamed (library panel inline edit). None = not renaming.
    pub renaming_source_id: Option<SourceId>,
    /// Scene currently being renamed (scenes panel inline edit). None = not renaming.
    pub renaming_scene_id: Option<SceneId>,
    /// Buffer for the inline rename text edit.
    pub rename_buffer: String,
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
    /// Resolved accent color from settings, cached to avoid parsing hex every frame.
    pub accent_color: egui::Color32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scenes: Vec::new(),
            library: Vec::new(),
            active_scene_id: None,
            selected_source_id: None,
            selected_library_source_id: None,
            flash_source_id: None,
            flash_start: None,
            renaming_source_id: None,
            renaming_scene_id: None,
            rename_buffer: String::new(),
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
            accent_color: crate::ui::theme::DEFAULT_ACCENT,
        }
    }
}

impl AppState {
    /// Find a library source by ID.
    #[allow(dead_code)]
    pub fn find_library_source(&self, id: SourceId) -> Option<&LibrarySource> {
        self.library.iter().find(|s| s.id == id)
    }

    /// Find a mutable library source by ID.
    #[allow(dead_code)]
    pub fn find_library_source_mut(&mut self, id: SourceId) -> Option<&mut LibrarySource> {
        self.library.iter_mut().find(|s| s.id == id)
    }

    /// Get the active scene.
    pub fn active_scene(&self) -> Option<&Scene> {
        self.active_scene_id
            .and_then(|id| self.scenes.iter().find(|s| s.id == id))
    }

    /// Get the active scene mutably.
    pub fn active_scene_mut(&mut self) -> Option<&mut Scene> {
        self.active_scene_id
            .and_then(|id| self.scenes.iter_mut().find(|s| s.id == id))
    }

    /// Count how many scenes reference a given source.
    pub fn source_usage_count(&self, source_id: SourceId) -> usize {
        self.scenes
            .iter()
            .filter(|s| s.sources.iter().any(|ss| ss.source_id == source_id))
            .count()
    }

    /// Get scene names that reference a given source.
    #[allow(dead_code)]
    pub fn scenes_using_source(&self, source_id: SourceId) -> Vec<String> {
        self.scenes
            .iter()
            .filter(|s| s.sources.iter().any(|ss| ss.source_id == source_id))
            .map(|s| s.name.clone())
            .collect()
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
