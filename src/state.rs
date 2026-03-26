use crate::gstreamer::{GstCommand, GstError};
use crate::scene::{LibrarySource, Scene, SceneId, SourceId, SourceOverrides};
use crate::settings::AppSettings;

// ── Undo / Redo ──────────────────────────────────────────────────────────────

const UNDO_MAX_DEPTH: usize = 50;

/// Snapshot of the undoable portion of app state.
#[derive(Clone, Debug)]
pub(crate) struct UndoSnapshot {
    scenes: Vec<Scene>,
    library: Vec<LibrarySource>,
    active_scene_id: Option<SceneId>,
    next_scene_id: u64,
    next_source_id: u64,
}

/// Snapshot-based undo/redo stack.
#[derive(Clone, Default)]
pub struct UndoStack {
    pub(crate) undo: Vec<UndoSnapshot>,
    pub(crate) redo: Vec<UndoSnapshot>,
}

impl std::fmt::Debug for UndoStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoStack")
            .field("undo_depth", &self.undo.len())
            .field("redo_depth", &self.redo.len())
            .finish()
    }
}

impl UndoStack {
    fn restore(snapshot: UndoSnapshot, state: &mut AppState) {
        state.scenes = snapshot.scenes;
        state.library = snapshot.library;
        state.active_scene_id = snapshot.active_scene_id;
        state.next_scene_id = snapshot.next_scene_id;
        state.next_source_id = snapshot.next_source_id;

        // Clear selections that reference sources/scenes that no longer exist.
        let active_source_ids: Vec<SourceId> = state
            .active_scene()
            .map(|s| s.source_ids())
            .unwrap_or_default();
        state
            .selected_source_ids
            .retain(|id| active_source_ids.contains(id));
        if let Some(primary) = state.primary_selected_id
            && !state.selected_source_ids.contains(&primary)
        {
            state.primary_selected_id = state.selected_source_ids.last().copied();
        }
        if let Some(id) = state.selected_library_source_id
            && !state.library.iter().any(|s| s.id == id)
        {
            state.selected_library_source_id = None;
        }

        state.mark_dirty_no_undo();
    }

    #[allow(dead_code)]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[allow(dead_code)]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
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

/// Whether the app is currently recording.
#[derive(Debug, Clone)]
pub enum RecordingStatus {
    Idle,
    Recording { path: std::path::PathBuf },
}

/// A clipboard entry for copy/paste of scene sources.
#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    pub library_source_id: SourceId,
    pub overrides_snapshot: SourceOverrides,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub library: Vec<LibrarySource>,
    pub active_scene_id: Option<SceneId>,
    /// Sources selected in the scene (sources panel / preview). Drives transform handles.
    pub selected_source_ids: Vec<SourceId>,
    /// The primary source in a multi-select (most recently clicked).
    pub primary_selected_id: Option<SourceId>,
    /// Source selected in the library panel. Drives properties panel in "Library Defaults" mode.
    pub selected_library_source_id: Option<SourceId>,
    /// Source to flash briefly in the scene (sources panel + preview) when selected in library.
    pub flash_source_id: Option<SourceId>,
    /// When the flash started, for animation timing.
    pub flash_start: Option<std::time::Instant>,
    /// Clipboard for copy/paste of scene sources.
    pub clipboard: Vec<ClipboardEntry>,
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
    /// Primary monitor resolution detected at startup via SCDisplay, in logical points.
    /// `None` if detection failed. Used by settings UI to offer detected resolution.
    pub detected_resolution: Option<(u32, u32)>,
    /// Available displays with resolution info, populated at startup.
    pub available_displays: Vec<crate::gstreamer::DisplayInfo>,
    pub capture_active: bool,
    pub active_errors: Vec<GstError>,
    pub recording_status: RecordingStatus,
    /// Encoders detected as available at startup by the GStreamer thread.
    pub available_encoders: Vec<crate::gstreamer::AvailableEncoder>,
    /// Timestamp when the current recording started, for elapsed-time display.
    pub recording_started_at: Option<std::time::Instant>,
    /// Monotonically incrementing counter used to generate unique recording filenames.
    pub recording_counter: u32,
    pub virtual_camera_active: bool,
    pub command_tx: Option<tokio::sync::mpsc::Sender<GstCommand>>,
    /// Resolved accent color from settings, cached to avoid parsing hex every frame.
    pub accent_color: egui::Color32,
    /// Undo/redo history stack.
    pub undo_stack: UndoStack,
    /// Signal from keyboard shortcut: reset preview zoom to fit (Cmd+0).
    pub reset_preview_zoom: bool,
    /// Signal from keyboard shortcut: set preview zoom to 1:1 pixel mapping (Cmd+1).
    pub set_preview_zoom_100: bool,
    /// Whether a continuous edit gesture is in progress (drag, slider).
    pub(crate) in_continuous_edit: bool,
    /// Pre-frame snapshot for undo. Captured at the start of each UI frame
    /// so `mark_dirty()` can push it as the undo point.
    pub(crate) frame_snapshot: Option<UndoSnapshot>,
    /// Timestamp of the last arrow-key nudge, for batching undo snapshots.
    pub last_nudge_time: Option<std::time::Instant>,
    /// Available system font families for the appearance settings UI.
    pub system_fonts: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scenes: Vec::new(),
            library: Vec::new(),
            active_scene_id: None,
            selected_source_ids: Vec::new(),
            primary_selected_id: None,
            selected_library_source_id: None,
            flash_source_id: None,
            flash_start: None,
            clipboard: Vec::new(),
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
            detected_resolution: None,
            available_displays: Vec::new(),
            capture_active: true,
            active_errors: Vec::new(),
            recording_status: RecordingStatus::Idle,
            available_encoders: Vec::new(),
            recording_started_at: None,
            recording_counter: 0,
            virtual_camera_active: false,
            command_tx: None,
            accent_color: crate::ui::theme::DEFAULT_ACCENT,
            undo_stack: UndoStack::default(),
            reset_preview_zoom: false,
            set_preview_zoom_100: false,
            in_continuous_edit: false,
            frame_snapshot: None,
            last_nudge_time: None,
            system_fonts: vec!["Default".to_string()],
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

    // ── Selection helpers ──────────────────────────────────────────────────

    /// Returns the primary selected source ID (backward compat).
    pub fn selected_source_id(&self) -> Option<SourceId> {
        self.primary_selected_id
    }

    /// Select a single source (clears multi-select).
    pub fn select_source(&mut self, id: SourceId) {
        self.selected_source_ids = vec![id];
        self.primary_selected_id = Some(id);
    }

    /// Deselect all sources.
    pub fn deselect_all(&mut self) {
        self.selected_source_ids.clear();
        self.primary_selected_id = None;
    }

    /// Toggle a source in the selection (for Shift+click).
    pub fn toggle_source_selection(&mut self, id: SourceId) {
        if let Some(pos) = self.selected_source_ids.iter().position(|&s| s == id) {
            self.selected_source_ids.remove(pos);
            if self.primary_selected_id == Some(id) {
                self.primary_selected_id = self.selected_source_ids.last().copied();
            }
        } else {
            self.selected_source_ids.push(id);
            self.primary_selected_id = Some(id);
        }
    }

    /// Check if a source is selected.
    pub fn is_source_selected(&self, id: SourceId) -> bool {
        self.selected_source_ids.contains(&id)
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

    // ── Undo-aware dirty marking ──────────────────────────────────────────

    // ── Undo-aware dirty marking ──────────────────────────────────────────

    /// Capture a snapshot of the current state at the start of each UI frame.
    /// If any mutation calls `mark_dirty()` during this frame, the snapshot
    /// is pushed onto the undo stack (capturing the pre-mutation state).
    /// Must be called once per frame before any UI code runs.
    pub fn begin_frame_for_undo(&mut self) {
        // If we're in a continuous edit, the snapshot was already captured
        // at the start of the gesture — don't overwrite it.
        if self.in_continuous_edit {
            return;
        }
        self.frame_snapshot = Some(UndoSnapshot {
            scenes: self.scenes.clone(),
            library: self.library.clone(),
            active_scene_id: self.active_scene_id,
            next_scene_id: self.next_scene_id,
            next_source_id: self.next_source_id,
        });
    }

    /// Mark the scene collection as dirty and push the pre-frame undo snapshot.
    ///
    /// This is the **single chokepoint** for all scene/library mutations.
    /// Call this instead of setting `scenes_dirty` directly so that every
    /// mutation is automatically undoable.
    pub fn mark_dirty(&mut self) {
        // Push a pre-mutation snapshot onto the undo stack — but only once
        // per user action. `frame_snapshot` is set by `begin_frame_for_undo()`
        // at the start of each UI frame and consumed here on the first mutation.
        //
        // For mutations outside the frame (keyboard handlers), frame_snapshot
        // may be None. In that case we capture one now — the keyboard handler
        // runs before the mutation, so current state IS the pre-mutation state.
        //
        // For continuous edits, frame_snapshot is None (consumed by the first
        // mark_dirty of the gesture), and `in_continuous_edit` prevents
        // begin_frame_for_undo from replacing it, so subsequent calls are no-ops.
        let snapshot = self.frame_snapshot.take();
        if let Some(snap) = snapshot {
            self.undo_stack.redo.clear();
            self.undo_stack.undo.push(snap);
            if self.undo_stack.undo.len() > UNDO_MAX_DEPTH {
                self.undo_stack.undo.remove(0);
            }
        } else if !self.in_continuous_edit && !self.scenes_dirty {
            // No frame snapshot and not in a continuous edit — this is a
            // mutation from outside the UI frame (keyboard handler, menu event).
            // Capture current state as the undo point.
            self.undo_stack.redo.clear();
            self.undo_stack.undo.push(UndoSnapshot {
                scenes: self.scenes.clone(),
                library: self.library.clone(),
                active_scene_id: self.active_scene_id,
                next_scene_id: self.next_scene_id,
                next_source_id: self.next_source_id,
            });
            if self.undo_stack.undo.len() > UNDO_MAX_DEPTH {
                self.undo_stack.undo.remove(0);
            }
        }
        self.scenes_dirty = true;
        self.scenes_last_changed = std::time::Instant::now();
    }

    /// Mark dirty without pushing an undo snapshot.
    /// Used only by the undo/redo restore path.
    pub fn mark_dirty_no_undo(&mut self) {
        self.scenes_dirty = true;
        self.scenes_last_changed = std::time::Instant::now();
    }

    /// Begin a continuous edit gesture (drag, slider).
    /// Takes the pre-frame snapshot so it's preserved for the entire gesture.
    /// Subsequent `mark_dirty()` calls during the gesture will not push
    /// additional snapshots.
    pub fn begin_continuous_edit(&mut self) {
        if !self.in_continuous_edit {
            // The frame_snapshot was captured at frame start — take() it so
            // mark_dirty() can push it on the first call, but further
            // begin_frame_for_undo() calls won't overwrite it.
            self.in_continuous_edit = true;
        }
    }

    /// End a continuous edit gesture. The next frame will capture a fresh
    /// snapshot.
    pub fn end_continuous_edit(&mut self) {
        self.in_continuous_edit = false;
    }

    /// Undo the last action. Returns `true` if state was restored.
    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.undo.pop() else {
            return false;
        };
        self.undo_stack.redo.push(UndoSnapshot {
            scenes: self.scenes.clone(),
            library: self.library.clone(),
            active_scene_id: self.active_scene_id,
            next_scene_id: self.next_scene_id,
            next_source_id: self.next_source_id,
        });
        UndoStack::restore(snapshot, self);
        true
    }

    /// Redo the last undone action. Returns `true` if state was restored.
    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.redo.pop() else {
            return false;
        };
        self.undo_stack.undo.push(UndoSnapshot {
            scenes: self.scenes.clone(),
            library: self.library.clone(),
            active_scene_id: self.active_scene_id,
            next_scene_id: self.next_scene_id,
            next_source_id: self.next_source_id,
        });
        UndoStack::restore(snapshot, self);
        true
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
    fn undo_restores_scene_after_source_removal() {
        use crate::scene::{Scene, SceneId, SceneSource, SourceId, SourceOverrides};
        let mut state = AppState::default();
        let scene_id = SceneId(1);
        let src_id = SourceId(10);
        state.scenes.push(Scene {
            id: scene_id,
            name: "Test".into(),
            sources: vec![SceneSource {
                source_id: src_id,
                overrides: SourceOverrides::default(),
            }],
            pinned: false,
        });
        state.active_scene_id = Some(scene_id);
        state.next_scene_id = 2;
        state.next_source_id = 11;

        // Simulate a frame start, then mutate, then mark dirty.
        state.begin_frame_for_undo();
        state.scenes[0].sources.clear();
        state.mark_dirty();

        assert!(state.scenes[0].sources.is_empty());

        // Undo should restore the source.
        assert!(state.undo());
        assert_eq!(state.scenes[0].sources.len(), 1);
        assert_eq!(state.scenes[0].sources[0].source_id, src_id);

        // Redo should remove it again.
        assert!(state.redo());
        assert!(state.scenes[0].sources.is_empty());
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

    // ── Selection helper tests ────────────────────────────────────────────

    #[test]
    fn select_source_sets_selection() {
        use crate::scene::SourceId;
        let mut state = AppState::default();
        let id = SourceId(42);
        state.select_source(id);
        assert!(state.is_source_selected(id));
        assert_eq!(state.primary_selected_id, Some(id));
        assert_eq!(state.selected_source_ids.len(), 1);
    }

    #[test]
    fn select_source_replaces_previous_selection() {
        use crate::scene::SourceId;
        let mut state = AppState::default();
        let a = SourceId(1);
        let b = SourceId(2);
        state.select_source(a);
        state.select_source(b);
        assert!(!state.is_source_selected(a));
        assert!(state.is_source_selected(b));
        assert_eq!(state.primary_selected_id, Some(b));
    }

    #[test]
    fn deselect_all_clears_selection() {
        use crate::scene::SourceId;
        let mut state = AppState::default();
        state.select_source(SourceId(1));
        state.deselect_all();
        assert!(state.selected_source_ids.is_empty());
        assert_eq!(state.primary_selected_id, None);
    }

    #[test]
    fn toggle_source_selection_adds_and_removes() {
        use crate::scene::SourceId;
        let mut state = AppState::default();
        let a = SourceId(10);
        let b = SourceId(20);

        // Toggle in two sources.
        state.toggle_source_selection(a);
        state.toggle_source_selection(b);
        assert!(state.is_source_selected(a));
        assert!(state.is_source_selected(b));
        assert_eq!(state.primary_selected_id, Some(b));

        // Toggle b out — primary should fall back to a.
        state.toggle_source_selection(b);
        assert!(!state.is_source_selected(b));
        assert!(state.is_source_selected(a));
        assert_eq!(state.primary_selected_id, Some(a));
    }

    #[test]
    fn is_source_selected_returns_false_when_empty() {
        use crate::scene::SourceId;
        let state = AppState::default();
        assert!(!state.is_source_selected(SourceId(99)));
    }
}
