# Lodestone MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working native streaming/recording application shell with custom GPU-rendered UI, mock OBS backend, and settings persistence.

**Architecture:** Monolithic render loop driven by `winit`. `wgpu` owns the GPU surface. `egui` handles layout/input only — all visuals rendered through custom SDF-based wgpu pipelines. `glyphon` renders all text. OBS functionality stubbed behind a trait with `MockObsEngine`.

**Tech Stack:** Rust 2024 edition, winit, wgpu, egui/egui-wgpu, glyphon, cosmic-text, tokio 1, anyhow, serde + toml, dirs

**Note:** Crate versions in this plan are approximate. Verify against crates.io at implementation time and adjust API calls if needed (especially for `wgpu` which has frequent breaking changes).

**Spec:** `docs/superpowers/specs/2026-03-19-lodestone-mvp-design.md`

---

## File Structure

```text
src/
├── main.rs                  ← entry point: winit event loop, wgpu init, app bootstrap
├── state.rs                 ← AppState struct, Scene, Source, AudioLevel, StreamStatus, UiState types
├── renderer/
│   ├── mod.rs               ← Renderer struct: owns wgpu device/queue/surface, orchestrates render passes
│   ├── pipelines.rs         ← SDF widget pipeline, blur pipeline: shaders, bind groups, draw calls
│   ├── text.rs              ← GlyphonRenderer: font atlas, text buffer management, render pass
│   └── preview.rs           ← PreviewRenderer: texture upload from RgbaFrame, fullscreen quad
├── ui/
│   ├── mod.rs               ← UiRoot: creates egui context, dispatches to panel modules, collects layout
│   ├── scene_editor.rs      ← scene list, source list, transform controls
│   ├── audio_mixer.rs       ← per-source faders, mute toggles, VU meters
│   └── stream_controls.rs   ← go live/stop, destination selector, live stats HUD
├── obs/
│   ├── mod.rs               ← ObsEngine trait, RgbaFrame, ObsStats, channel types
│   ├── mock.rs              ← MockObsEngine: fake scenes/sources/audio/stats
│   ├── scene.rs             ← SceneId, SourceId, Scene, Source, SourceConfig, Transform types
│   ├── output.rs            ← StreamConfig, StreamDestination types
│   └── encoder.rs           ← EncoderConfig type
├── settings.rs              ← AppSettings, ProfileSettings, TOML load/save, dirs-based paths
└── mock_driver.rs           ← tokio task: animated mock audio levels and stream stats
```

---

### Task 1: Project Setup & Dependencies

**Files:**
- Modify: `Cargo.toml`
- Create: `src/main.rs` (replace placeholder)

- [ ] **Step 1: Update Cargo.toml with all dependencies**

```toml
[package]
name = "lodestone"
version = "0.1.0"
edition = "2024"

[dependencies]
winit = "0.30"
wgpu = "28.0"
egui = "0.33"
egui-wgpu = "0.33"
egui-winit = "0.33"
glyphon = "0.5"
cosmic-text = "0.15"
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
toml = "0.9"
dirs = "6.0"
log = "0.4"
env_logger = "0.11"
rand = "0.9"
bytemuck = { version = "1", features = ["derive"] }
```

- [ ] **Step 2: Replace main.rs with minimal skeleton**

```rust
use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    Ok(())
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles with no errors (warnings about unused deps are fine)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "Add project dependencies and minimal entry point"
```

---

### Task 2: Core Types — OBS Scene/Source/Output

**Files:**
- Create: `src/obs/mod.rs`
- Create: `src/obs/scene.rs`
- Create: `src/obs/output.rs`
- Create: `src/obs/encoder.rs`

- [ ] **Step 1: Write tests for scene/source types**

Create `src/obs/scene.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SourceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    pub transform: Transform,
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceType {
    Display,
    Window,
    Camera,
    Audio,
    Image,
    Browser,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub name: String,
    pub source_type: SourceType,
    pub transform: Transform,
}

impl Transform {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_stores_source_ids() {
        let scene = Scene {
            id: SceneId(1),
            name: "Main".to_string(),
            sources: vec![SourceId(10), SourceId(20)],
        };
        assert_eq!(scene.sources.len(), 2);
        assert_eq!(scene.sources[0], SourceId(10));
    }

    #[test]
    fn transform_constructor() {
        let t = Transform::new(100.0, 200.0, 1920.0, 1080.0);
        assert_eq!(t.x, 100.0);
        assert_eq!(t.width, 1920.0);
    }

    #[test]
    fn source_defaults_visible_unmuted() {
        let source = Source {
            id: SourceId(1),
            name: "Webcam".to_string(),
            source_type: SourceType::Camera,
            transform: Transform::new(0.0, 0.0, 640.0, 480.0),
            visible: true,
            muted: false,
            volume: 1.0,
        };
        assert!(source.visible);
        assert!(!source.muted);
        assert_eq!(source.volume, 1.0);
    }
}
```

- [ ] **Step 2: Create output types**

Create `src/obs/output.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub destination: StreamDestination,
    pub stream_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StreamDestination {
    Twitch,
    YouTube,
    CustomRtmp { url: String },
}

impl StreamDestination {
    pub fn rtmp_url(&self) -> &str {
        match self {
            Self::Twitch => "rtmp://live.twitch.tv/app",
            Self::YouTube => "rtmp://a.rtmp.youtube.com/live2",
            Self::CustomRtmp { url } => url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_rtmp_url() {
        assert_eq!(StreamDestination::Twitch.rtmp_url(), "rtmp://live.twitch.tv/app");
    }

    #[test]
    fn custom_rtmp_url() {
        let dest = StreamDestination::CustomRtmp { url: "rtmp://my.server/live".to_string() };
        assert_eq!(dest.rtmp_url(), "rtmp://my.server/live");
    }
}
```

- [ ] **Step 3: Create encoder types**

Create `src/obs/encoder.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_encoder_config() {
        let config = EncoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 30);
        assert_eq!(config.bitrate_kbps, 4500);
    }
}
```

- [ ] **Step 4: Create obs module root**

Create `src/obs/mod.rs`:

```rust
pub mod encoder;
pub mod mock;
pub mod output;
pub mod scene;

use std::path::Path;

use anyhow::Result;
use tokio::sync::mpsc::Receiver;

pub use encoder::EncoderConfig;
pub use output::{StreamConfig, StreamDestination};
pub use scene::{Scene, SceneId, Source, SourceConfig, SourceId, SourceType, Transform};

/// Statistics emitted by the OBS engine.
#[derive(Debug, Clone)]
pub struct ObsStats {
    pub bitrate_kbps: f64,
    pub dropped_frames: u64,
    pub total_frames: u64,
    pub uptime_secs: f64,
}

/// CPU-side RGBA frame buffer. The renderer uploads this to a wgpu::Texture.
#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Abstraction over the OBS engine. MockObsEngine for development,
/// LiveObsEngine (future) for real libobs integration.
pub trait ObsEngine {
    fn scenes(&self) -> Vec<Scene>;
    fn create_scene(&mut self, name: &str) -> Result<SceneId>;
    fn remove_scene(&mut self, id: SceneId) -> Result<()>;
    fn set_active_scene(&mut self, id: SceneId) -> Result<()>;
    fn add_source(&mut self, scene: SceneId, source: SourceConfig) -> Result<SourceId>;
    fn remove_source(&mut self, scene: SceneId, source: SourceId) -> Result<()>;
    fn update_source_transform(&mut self, source: SourceId, transform: Transform) -> Result<()>;

    fn set_volume(&mut self, source: SourceId, volume: f32) -> Result<()>;
    fn set_muted(&mut self, source: SourceId, muted: bool) -> Result<()>;

    fn start_stream(&mut self, config: StreamConfig) -> Result<()>;
    fn stop_stream(&mut self) -> Result<()>;
    fn start_recording(&mut self, path: &Path) -> Result<()>;
    fn stop_recording(&mut self) -> Result<()>;

    fn configure_encoder(&mut self, config: EncoderConfig) -> Result<()>;

    fn subscribe_stats(&self) -> Receiver<ObsStats>;
    fn get_frame(&self) -> Option<RgbaFrame>;
}
```

Note: `pub mod mock;` will cause a compile error until Task 3 creates `mock.rs`. Add it commented out for now, uncomment in Task 3.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib obs`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/obs/
git commit -m "Add OBS abstraction types: scenes, sources, output, encoder"
```

---

### Task 3: MockObsEngine

**Files:**
- Create: `src/obs/mock.rs`
- Modify: `src/obs/mod.rs` (uncomment `pub mod mock;`)

- [ ] **Step 1: Write tests for MockObsEngine**

Tests go at the bottom of `src/obs/mock.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_default_scene() {
        let engine = MockObsEngine::new();
        let scenes = engine.scenes();
        assert_eq!(scenes.len(), 1);
        assert_eq!(scenes[0].name, "Scene 1");
    }

    #[test]
    fn create_and_remove_scene() {
        let mut engine = MockObsEngine::new();
        let id = engine.create_scene("Test Scene").unwrap();
        assert_eq!(engine.scenes().len(), 2);

        engine.remove_scene(id).unwrap();
        assert_eq!(engine.scenes().len(), 1);
    }

    #[test]
    fn remove_nonexistent_scene_errors() {
        let mut engine = MockObsEngine::new();
        assert!(engine.remove_scene(SceneId(999)).is_err());
    }

    #[test]
    fn add_source_to_scene() {
        let mut engine = MockObsEngine::new();
        let scenes = engine.scenes();
        let scene_id = scenes[0].id;

        let source_id = engine.add_source(scene_id, SourceConfig {
            name: "Webcam".to_string(),
            source_type: SourceType::Camera,
            transform: Transform::new(0.0, 0.0, 640.0, 480.0),
        }).unwrap();

        let scenes = engine.scenes();
        assert!(scenes[0].sources.contains(&source_id));
    }

    #[test]
    fn set_volume_and_mute() {
        let mut engine = MockObsEngine::new();
        let scenes = engine.scenes();
        let scene_id = scenes[0].id;

        let source_id = engine.add_source(scene_id, SourceConfig {
            name: "Mic".to_string(),
            source_type: SourceType::Audio,
            transform: Transform::new(0.0, 0.0, 0.0, 0.0),
        }).unwrap();

        engine.set_volume(source_id, 0.5).unwrap();
        engine.set_muted(source_id, true).unwrap();

        let source = engine.get_source(source_id).unwrap();
        assert_eq!(source.volume, 0.5);
        assert!(source.muted);
    }

    #[test]
    fn active_scene_management() {
        let mut engine = MockObsEngine::new();
        let scene2 = engine.create_scene("Scene 2").unwrap();

        engine.set_active_scene(scene2).unwrap();
        assert_eq!(engine.active_scene_id(), Some(scene2));
    }

    #[test]
    fn stream_start_stop() {
        let mut engine = MockObsEngine::new();
        assert!(!engine.is_streaming());

        engine.start_stream(StreamConfig {
            destination: StreamDestination::Twitch,
            stream_key: "live_test_key".to_string(),
        }).unwrap();
        assert!(engine.is_streaming());

        engine.stop_stream().unwrap();
        assert!(!engine.is_streaming());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib obs::mock`
Expected: FAIL — `MockObsEngine` does not exist yet

- [ ] **Step 3: Implement MockObsEngine**

Write `src/obs/mock.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use anyhow::{Result, bail};
use tokio::sync::mpsc::{self, Receiver};

use super::{
    EncoderConfig, ObsEngine, ObsStats, RgbaFrame, Scene, SceneId, Source, SourceConfig, SourceId,
    StreamConfig, Transform,
    scene::SourceType,
};

/// Mock implementation of ObsEngine for UI development without libobs.
pub struct MockObsEngine {
    scenes: HashMap<SceneId, Scene>,
    sources: HashMap<SourceId, Source>,
    active_scene: Option<SceneId>,
    next_scene_id: u64,
    next_source_id: u64,
    streaming: bool,
    recording: bool,
    encoder_config: EncoderConfig,
}

impl MockObsEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            scenes: HashMap::new(),
            sources: HashMap::new(),
            active_scene: None,
            next_scene_id: 1,
            next_source_id: 1,
            streaming: false,
            recording: false,
            encoder_config: EncoderConfig::default(),
        };

        // Create a default scene
        let id = engine.create_scene("Scene 1").expect("default scene creation");
        engine.active_scene = Some(id);
        engine
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn active_scene_id(&self) -> Option<SceneId> {
        self.active_scene
    }

    pub fn get_source(&self, id: SourceId) -> Option<&Source> {
        self.sources.get(&id)
    }

    fn alloc_scene_id(&mut self) -> SceneId {
        let id = SceneId(self.next_scene_id);
        self.next_scene_id += 1;
        id
    }

    fn alloc_source_id(&mut self) -> SourceId {
        let id = SourceId(self.next_source_id);
        self.next_source_id += 1;
        id
    }
}

impl ObsEngine for MockObsEngine {
    fn scenes(&self) -> Vec<Scene> {
        self.scenes.values().cloned().collect()
    }

    fn create_scene(&mut self, name: &str) -> Result<SceneId> {
        let id = self.alloc_scene_id();
        self.scenes.insert(id, Scene {
            id,
            name: name.to_string(),
            sources: Vec::new(),
        });
        Ok(id)
    }

    fn remove_scene(&mut self, id: SceneId) -> Result<()> {
        if self.scenes.remove(&id).is_none() {
            bail!("scene {id:?} not found");
        }
        if self.active_scene == Some(id) {
            self.active_scene = self.scenes.keys().next().copied();
        }
        Ok(())
    }

    fn set_active_scene(&mut self, id: SceneId) -> Result<()> {
        if !self.scenes.contains_key(&id) {
            bail!("scene {id:?} not found");
        }
        self.active_scene = Some(id);
        Ok(())
    }

    fn add_source(&mut self, scene: SceneId, config: SourceConfig) -> Result<SourceId> {
        let scene_entry = self.scenes.get_mut(&scene)
            .ok_or_else(|| anyhow::anyhow!("scene {scene:?} not found"))?;

        let id = self.alloc_source_id();
        let source = Source {
            id,
            name: config.name,
            source_type: config.source_type,
            transform: config.transform,
            visible: true,
            muted: false,
            volume: 1.0,
        };
        self.sources.insert(id, source);
        scene_entry.sources.push(id);
        Ok(id)
    }

    fn remove_source(&mut self, scene: SceneId, source: SourceId) -> Result<()> {
        let scene_entry = self.scenes.get_mut(&scene)
            .ok_or_else(|| anyhow::anyhow!("scene {scene:?} not found"))?;

        scene_entry.sources.retain(|&s| s != source);
        self.sources.remove(&source);
        Ok(())
    }

    fn update_source_transform(&mut self, source: SourceId, transform: Transform) -> Result<()> {
        let s = self.sources.get_mut(&source)
            .ok_or_else(|| anyhow::anyhow!("source {source:?} not found"))?;
        s.transform = transform;
        Ok(())
    }

    fn set_volume(&mut self, source: SourceId, volume: f32) -> Result<()> {
        let s = self.sources.get_mut(&source)
            .ok_or_else(|| anyhow::anyhow!("source {source:?} not found"))?;
        s.volume = volume;
        Ok(())
    }

    fn set_muted(&mut self, source: SourceId, muted: bool) -> Result<()> {
        let s = self.sources.get_mut(&source)
            .ok_or_else(|| anyhow::anyhow!("source {source:?} not found"))?;
        s.muted = muted;
        Ok(())
    }

    fn start_stream(&mut self, _config: StreamConfig) -> Result<()> {
        self.streaming = true;
        log::info!("Mock: stream started");
        Ok(())
    }

    fn stop_stream(&mut self) -> Result<()> {
        self.streaming = false;
        log::info!("Mock: stream stopped");
        Ok(())
    }

    fn start_recording(&mut self, path: &Path) -> Result<()> {
        self.recording = true;
        log::info!("Mock: recording to {}", path.display());
        Ok(())
    }

    fn stop_recording(&mut self) -> Result<()> {
        self.recording = false;
        log::info!("Mock: recording stopped");
        Ok(())
    }

    fn configure_encoder(&mut self, config: EncoderConfig) -> Result<()> {
        self.encoder_config = config;
        Ok(())
    }

    fn subscribe_stats(&self) -> Receiver<ObsStats> {
        let (_tx, rx) = mpsc::channel(16);
        // Mock stats are driven by the mock data driver (Task 12), not here
        rx
    }

    fn get_frame(&self) -> Option<RgbaFrame> {
        // Return a solid dark gray frame
        let w = self.encoder_config.width;
        let h = self.encoder_config.height;
        let data = vec![30; (w * h * 4) as usize]; // dark gray RGBA
        Some(RgbaFrame { data, width: w, height: h })
    }
}
```

- [ ] **Step 4: Ensure `pub mod mock;` is uncommented in `src/obs/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test --lib obs`
Expected: all tests pass

- [ ] **Step 6: Commit**

```bash
git add src/obs/
git commit -m "Implement MockObsEngine with scene/source CRUD and stream control"
```

---

### Task 5: AppState & State Types

**Depends on:** Task 4 (settings types must exist for `AppState.settings` field)

**Files:**
- Create: `src/state.rs`

- [ ] **Step 1: Write tests for AppState**

Tests at bottom of `src/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_state() {
        let state = AppState::default();
        assert!(matches!(state.stream_status, StreamStatus::Offline));
        assert!(state.ui_state.scene_panel_open);
        assert!(state.ui_state.mixer_panel_open);
        assert!(state.ui_state.controls_panel_open);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib state`
Expected: FAIL — module does not exist

- [ ] **Step 3: Implement state types**

Write `src/state.rs`:

```rust
use crate::obs::{Scene, SceneId, Source, SourceId};
use crate::settings::AppSettings;

/// Per-source audio metering data.
#[derive(Debug, Clone)]
pub struct AudioLevel {
    pub source_id: SourceId,
    pub current_db: f32,
    pub peak_db: f32,
}

impl AudioLevel {
    pub fn new(source_id: SourceId, current_db: f32, peak_db: f32) -> Self {
        Self {
            source_id,
            current_db: current_db.max(-60.0).min(0.0),
            peak_db: peak_db.max(-60.0).min(0.0),
        }
    }
}

/// Current streaming/recording status.
#[derive(Debug, Clone)]
pub enum StreamStatus {
    Offline,
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

/// Tracks which UI panels are open/collapsed.
#[derive(Debug, Clone)]
pub struct UiState {
    pub scene_panel_open: bool,
    pub mixer_panel_open: bool,
    pub controls_panel_open: bool,
    pub settings_modal_open: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            scene_panel_open: true,
            mixer_panel_open: true,
            controls_panel_open: true,
            settings_modal_open: false,
        }
    }
}

/// Root application state. Shared via Arc<Mutex<AppState>>.
#[derive(Debug, Clone)]
pub struct AppState {
    pub scenes: Vec<Scene>,
    pub sources: Vec<Source>,
    pub active_scene_id: Option<SceneId>,
    pub audio_levels: Vec<AudioLevel>,
    pub stream_status: StreamStatus,
    pub settings: AppSettings,
    pub ui_state: UiState,
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
            ui_state: UiState::default(),
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
        assert!(state.ui_state.scene_panel_open);
        assert!(state.ui_state.mixer_panel_open);
        assert!(state.ui_state.controls_panel_open);
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
```

- [ ] **Step 4: Add `mod state;` to `main.rs`**

Update `src/main.rs` (should already have `mod obs;` and `mod settings;` from previous tasks):

```rust
mod obs;
mod settings;
mod state;

use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");
    Ok(())
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib`
Expected: all obs and state tests pass

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/main.rs
git commit -m "Add AppState with stream status, audio levels, and UI state tracking"
```

---

### Task 4: Settings Persistence

**Files:**
- Create: `src/settings.rs`

- [ ] **Step 1: Write tests for settings serialization**

Tests at bottom of `src/settings.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn default_settings_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.active_profile, settings.active_profile);
    }

    #[test]
    fn profile_roundtrip() {
        let profile = ProfileSettings {
            name: "Streaming".to_string(),
            destination: StreamDestination::Twitch,
            stream_key: "live_abc123".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        };
        let toml_str = toml::to_string_pretty(&profile).unwrap();
        let parsed: ProfileSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "Streaming");
        assert_eq!(parsed.bitrate_kbps, 4500);
        assert!(matches!(parsed.destination, StreamDestination::Twitch));
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let settings = AppSettings::load_from(Path::new("/nonexistent/path/settings.toml"));
        assert_eq!(settings.active_profile, "Default");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        file.write_all(toml_str.as_bytes()).unwrap();

        let loaded = AppSettings::load_from(file.path());
        assert_eq!(loaded.active_profile, settings.active_profile);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib settings`
Expected: FAIL — module does not exist

- [ ] **Step 3: Add `tempfile` dev dependency**

Add to `Cargo.toml`:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Implement settings**

Write `src/settings.rs`:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::obs::StreamDestination;

/// Global application settings. Persisted to TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub active_profile: String,
    pub ui: UiSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub scene_panel_open: bool,
    pub mixer_panel_open: bool,
    pub controls_panel_open: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            active_profile: "Default".to_string(),
            ui: UiSettings {
                scene_panel_open: true,
                mixer_panel_open: true,
                controls_panel_open: true,
            },
        }
    }
}

impl AppSettings {
    /// Load settings from a specific path. Returns defaults if file doesn't exist or is invalid.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save settings to a specific path.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }
}

/// Per-profile stream settings. One file per profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettings {
    pub name: String,
    pub destination: StreamDestination,
    pub stream_key: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            destination: StreamDestination::Twitch,
            stream_key: String::new(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        }
    }
}

/// Returns the platform-appropriate config directory for Lodestone.
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lodestone")
}

/// Returns the path to the main settings file.
pub fn settings_path() -> PathBuf {
    config_dir().join("settings.toml")
}

/// Returns the path to a specific profile file.
pub fn profile_path(name: &str) -> PathBuf {
    config_dir().join("profiles").join(format!("{name}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn default_settings_roundtrip() {
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        let parsed: AppSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.active_profile, settings.active_profile);
    }

    #[test]
    fn profile_roundtrip() {
        let profile = ProfileSettings {
            name: "Streaming".to_string(),
            destination: StreamDestination::Twitch,
            stream_key: "live_abc123".to_string(),
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
        };
        let toml_str = toml::to_string_pretty(&profile).unwrap();
        let parsed: ProfileSettings = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.name, "Streaming");
        assert_eq!(parsed.bitrate_kbps, 4500);
        assert!(matches!(parsed.destination, StreamDestination::Twitch));
    }

    #[test]
    fn load_nonexistent_returns_default() {
        let settings = AppSettings::load_from(Path::new("/nonexistent/path/settings.toml"));
        assert_eq!(settings.active_profile, "Default");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut file = NamedTempFile::new().unwrap();
        let settings = AppSettings::default();
        let toml_str = toml::to_string_pretty(&settings).unwrap();
        file.write_all(toml_str.as_bytes()).unwrap();

        let loaded = AppSettings::load_from(file.path());
        assert_eq!(loaded.active_profile, settings.active_profile);
    }
}
```

- [ ] **Step 5: Add `mod settings;` to `main.rs`**

- [ ] **Step 6: Run tests**

Run: `cargo test --lib settings`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/settings.rs src/main.rs Cargo.toml
git commit -m "Add settings persistence with TOML serialization and platform-aware paths"
```

---

### Task 6: winit + wgpu Shell

**Files:**
- Modify: `src/main.rs`
- Create: `src/renderer/mod.rs`

This task is GPU-dependent. No automated tests — verify by running the app and seeing a window with a dark background.

- [ ] **Step 1: Create renderer module with wgpu state**

Write `src/renderer/mod.rs`:

```rust
pub mod pipelines;
pub mod preview;
pub mod text;

use anyhow::Result;
use wgpu::{
    Device, Instance, Queue, Surface, SurfaceConfiguration, TextureFormat,
};
use winit::window::Window;

/// Owns all GPU state and orchestrates render passes.
pub struct Renderer {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'static>,
    pub surface_config: SurfaceConfiguration,
    pub format: TextureFormat,
}

impl Renderer {
    /// Initialize wgpu with the given window.
    pub async fn new(window: &'static Window) -> Result<Self> {
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("no suitable GPU adapter found"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("lodestone_device"),
                ..Default::default()
            })
            .await?;

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            format,
        })
    }

    /// Handle window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Render a frame. For now, just clears to a dark background.
    pub fn render(&mut self) -> Result<()> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });

        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.10,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
```

- [ ] **Step 2: Create placeholder submodules**

Create `src/renderer/pipelines.rs`:

```rust
// SDF widget pipeline and blur pipeline — implemented in Task 9
```

Create `src/renderer/text.rs`:

```rust
// Glyphon text rendering — implemented in Task 8
```

Create `src/renderer/preview.rs`:

```rust
// Preview texture pipeline — implemented in Task 10
```

- [ ] **Step 3: Add `pollster` dependency**

Add to `Cargo.toml`:

```toml
pollster = "0.4"
```

- [ ] **Step 4: Wire up main.rs with winit event loop**

Replace `src/main.rs`:

```rust
mod obs;
mod renderer;
mod settings;
mod state;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use renderer::Renderer;
use state::AppState;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::EventLoop,
    window::{Window, WindowAttributes},
};

struct App {
    window: Option<&'static Window>,
    renderer: Option<Renderer>,
    state: Arc<Mutex<AppState>>,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            state: Arc::new(Mutex::new(AppState::default())),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Lodestone")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_min_inner_size(LogicalSize::new(960.0, 540.0));

        let window = event_loop.create_window(attrs).expect("create window");
        // Leak the window so it lives for 'static — required by wgpu surface
        let window: &'static Window = Box::leak(Box::new(window));
        self.window = Some(window);

        let renderer = pollster::block_on(Renderer::new(window))
            .expect("initialize renderer");
        self.renderer = Some(renderer);

        log::info!("Window and renderer initialized");
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(width, height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    if let Err(e) = renderer.render() {
                        log::error!("Render error: {e}");
                    }
                }
                if let Some(window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    log::info!("Lodestone starting");

    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}
```

- [ ] **Step 5: Verify it compiles and runs**

Run: `cargo run`
Expected: a 1280x720 window opens with a dark background (rgb ~0.08, 0.08, 0.10). Window is resizable. Closing the window exits cleanly.

- [ ] **Step 6: Commit**

```bash
git add src/renderer/ src/main.rs Cargo.toml
git commit -m "Add winit + wgpu shell with dark background clear pass"
```

---

### Task 7: egui-wgpu Integration

**Files:**
- Modify: `src/renderer/mod.rs`
- Create: `src/ui/mod.rs`
- Modify: `src/main.rs`

No automated tests — verify visually.

- [ ] **Step 1: Create UI root module**

Write `src/ui/mod.rs`:

```rust
pub mod audio_mixer;
pub mod scene_editor;
pub mod stream_controls;

use egui::Context;

use crate::state::AppState;

/// Root UI layout. Dispatches to panel modules.
pub struct UiRoot {
    pub ctx: Context,
}

impl UiRoot {
    pub fn new() -> Self {
        Self {
            ctx: Context::default(),
        }
    }

    /// Run the egui layout pass. Returns the full output for rendering.
    pub fn run(&self, state: &mut AppState, raw_input: egui::RawInput) -> egui::FullOutput {
        self.ctx.run(raw_input, |ctx| {
            // For now, just a test panel to verify egui integration
            egui::Window::new("Lodestone")
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("egui integration working");
                    let status = match &state.stream_status {
                        crate::state::StreamStatus::Offline => "Offline",
                        crate::state::StreamStatus::Connecting => "Connecting...",
                        crate::state::StreamStatus::Live { .. } => "Live",
                    };
                    ui.label(format!("Status: {status}"));
                });
        })
    }
}
```

- [ ] **Step 2: Create placeholder UI submodules**

Create `src/ui/scene_editor.rs`:

```rust
// Scene editor panel — implemented in Task 11
```

Create `src/ui/audio_mixer.rs`:

```rust
// Audio mixer panel — implemented in Task 12
```

Create `src/ui/stream_controls.rs`:

```rust
// Stream controls panel — implemented in Task 13
```

- [ ] **Step 3: Add egui-wgpu rendering to Renderer**

Update `src/renderer/mod.rs`:

Add field to `Renderer`:

```rust
pub egui_renderer: egui_wgpu::Renderer,
```

Initialize in `new()` after device creation:

```rust
let egui_renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);
```

Add a new render method that replaces `render()`:

```rust
pub fn render_with_egui(
    &mut self,
    full_output: egui::FullOutput,
    screen: egui_wgpu::ScreenDescriptor,
) -> Result<()> {
    let output = self.surface.get_current_texture()?;
    let view = output.texture.create_view(&Default::default());

    // Handle texture delta (font textures, etc.)
    for (id, image_delta) in &full_output.textures_delta.set {
        self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
    }

    let clipped_primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);

    let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("render_encoder"),
    });

    // Update egui buffers
    self.egui_renderer.update_buffers(
        &self.device,
        &self.queue,
        &mut encoder,
        &clipped_primitives,
        &screen,
    );

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.08, g: 0.08, b: 0.10, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Render egui primitives
        self.egui_renderer.render(&mut render_pass, &clipped_primitives, &screen);
    }

    // Free released textures
    for id in &full_output.textures_delta.free {
        self.egui_renderer.free_texture(id);
    }

    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    Ok(())
}
```

Note: The `self.ctx` reference above is the `egui::Context` — you may need to pass it as a parameter or store it in `Renderer`. The exact API may differ based on `egui-wgpu` version; consult the `egui-wgpu` examples if the method signatures don't match.

- [ ] **Step 4: Wire egui into the main event loop**

Update `src/main.rs`:

Add to `App` struct:

```rust
ui: Option<UiRoot>,
egui_state: Option<egui_winit::State>,
```

In `resumed()`, after renderer init:

```rust
let egui_state = egui_winit::State::new(
    ui.ctx.clone(),
    egui::ViewportId::ROOT,
    &window,
    Some(window.scale_factor() as f32),
    None,
    None,
);
self.ui = Some(UiRoot::new());
self.egui_state = Some(egui_state);
```

In `window_event()`, feed events to egui before matching:

```rust
if let (Some(egui_state), Some(window)) = (&mut self.egui_state, self.window) {
    let _ = egui_state.on_window_event(window, &event);
}
```

In `RedrawRequested`, build UI and render:

```rust
WindowEvent::RedrawRequested => {
    if let (Some(ui), Some(egui_state), Some(renderer), Some(window)) =
        (&self.ui, &mut self.egui_state, &mut self.renderer, self.window)
    {
        let raw_input = egui_state.take_egui_input(window);
        let mut state = self.state.lock().expect("lock state");
        let full_output = ui.run(&mut state, raw_input);
        drop(state); // release lock before rendering

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [renderer.surface_config.width, renderer.surface_config.height],
            pixels_per_point: window.scale_factor() as f32,
        };

        egui_state.handle_platform_output(window, full_output.platform_output.clone());

        if let Err(e) = renderer.render_with_egui(full_output, screen) {
            log::error!("Render error: {e}");
        }
    }
    if let Some(window) = self.window {
        window.request_redraw();
    }
}
```

Add `mod ui;` declaration at the top.

- [ ] **Step 5: Verify it compiles and runs**

Run: `cargo run`
Expected: same dark window, now with a small egui window showing "egui integration working" and "Status: Offline". The egui window should be draggable and respond to input.

- [ ] **Step 6: Commit**

```bash
git add src/ui/ src/renderer/mod.rs src/main.rs
git commit -m "Integrate egui-wgpu for layout and input routing"
```

---

### Task 8: Glyphon Text Rendering

**Files:**
- Modify: `src/renderer/text.rs`
- Modify: `src/renderer/mod.rs`

No automated tests — verify visually that text renders with higher quality than egui's default.

- [ ] **Step 1: Implement GlyphonRenderer**

Write `src/renderer/text.rs`:

```rust
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer,
};

/// Describes a piece of text to render.
pub struct TextSection {
    pub text: String,
    pub position: [f32; 2],
    pub size: f32,
    pub color: [u8; 4], // RGBA
}

/// GPU text renderer using glyphon + cosmic-text.
pub struct GlyphonRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    buffers: Vec<Buffer>,
}

impl GlyphonRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let mut atlas = TextAtlas::new(device, queue, format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );

        Self {
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            buffers: Vec::new(),
        }
    }

    /// Prepare text sections for rendering. Call before begin_render_pass.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        sections: &[TextSection],
    ) -> anyhow::Result<()> {
        self.buffers.clear();

        let mut text_areas = Vec::new();

        for section in sections {
            let mut buffer = Buffer::new(
                &mut self.font_system,
                Metrics::new(section.size, section.size * 1.2),
            );
            buffer.set_size(&mut self.font_system, Some(width as f32), Some(height as f32));
            buffer.set_text(
                &mut self.font_system,
                &section.text,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);
            self.buffers.push(buffer);
        }

        for (i, section) in sections.iter().enumerate() {
            text_areas.push(TextArea {
                buffer: &self.buffers[i],
                left: section.position[0],
                top: section.position[1],
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
                default_color: Color::rgba(
                    section.color[0],
                    section.color[1],
                    section.color[2],
                    section.color[3],
                ),
                custom_glyphs: &[],
            });
        }

        self.text_renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &Resolution { width, height },
            text_areas,
            &mut self.swash_cache,
        )?;

        Ok(())
    }

    /// Render prepared text into the given render pass.
    pub fn render<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) -> anyhow::Result<()> {
        self.text_renderer.render(&self.atlas, render_pass)?;
        Ok(())
    }
}
```

Note: The glyphon API changes between versions. The above is structured for glyphon 0.5 — verify method signatures against the actual version and adjust. Consult the `glyphon` examples in the repo for reference.

- [ ] **Step 2: Integrate into the render loop**

Update `src/renderer/mod.rs`:
- Add `GlyphonRenderer` as a field of `Renderer`
- Call `text_renderer.prepare()` before the render pass
- Call `text_renderer.render()` as the last sub-pass (text renders on top of everything)

- [ ] **Step 3: Add a test label to verify rendering**

In the render method, add a test text section: "Lodestone" at position (20, 20), size 24, white.

- [ ] **Step 4: Verify it runs**

Run: `cargo run`
Expected: "Lodestone" text visible in top-left, rendered with subpixel quality (smoother than egui's default text). The egui test panel should also still work.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/text.rs src/renderer/mod.rs
git commit -m "Add glyphon text rendering pass with font system initialization"
```

---

### Task 9: SDF Widget Pipeline

**Files:**
- Modify: `src/renderer/pipelines.rs`
- Modify: `src/renderer/mod.rs`

No automated tests — verify visually. This is the most GPU-intensive task. Reference `wgpu` examples for pipeline setup patterns.

- [ ] **Step 1: Write SDF widget WGSL shader**

Create the shader as a `const` string in `src/renderer/pipelines.rs`. The shader implements a rounded rectangle SDF in a single pass:

```wgsl
struct WidgetParams {
    rect: vec4<f32>,         // x, y, width, height (pixels)
    color: vec4<f32>,        // fill color RGBA
    border_color: vec4<f32>, // border color RGBA
    corner_radius: f32,
    border_width: f32,
    shadow_offset: vec2<f32>,
    shadow_blur: f32,
    shadow_color: vec4<f32>,
    viewport_size: vec2<f32>,
    _padding: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var<uniform> params: WidgetParams;

// Fullscreen triangle trick — 3 vertices, no vertex buffer needed
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    // Expand rect to include shadow padding
    let padding = params.shadow_blur + length(params.shadow_offset);
    let rect = vec4<f32>(
        params.rect.x - padding,
        params.rect.y - padding,
        params.rect.z + padding * 2.0,
        params.rect.w + padding * 2.0,
    );

    // Generate quad from vertex index (0-5 for two triangles)
    let x = f32(idx & 1u);
    let y = f32((idx >> 1u) & 1u);
    let pos = vec2<f32>(
        rect.x + x * rect.z,
        rect.y + y * rect.w,
    );

    // Convert pixel coords to clip space
    out.position = vec4<f32>(
        pos.x / params.viewport_size.x * 2.0 - 1.0,
        1.0 - pos.y / params.viewport_size.y * 2.0,
        0.0, 1.0,
    );
    out.uv = pos;
    return out;
}

// Rounded rect SDF
fn sdf_rounded_rect(p: vec2<f32>, center: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let d = abs(p - center) - half_size + vec2<f32>(radius);
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let center = params.rect.xy + params.rect.zw * 0.5;
    let half_size = params.rect.zw * 0.5;

    // Shadow
    let shadow_center = center + params.shadow_offset;
    let shadow_dist = sdf_rounded_rect(in.uv, shadow_center, half_size, params.corner_radius);
    let shadow_alpha = 1.0 - smoothstep(-params.shadow_blur, params.shadow_blur, shadow_dist);
    let shadow = params.shadow_color * vec4<f32>(1.0, 1.0, 1.0, shadow_alpha);

    // Fill
    let dist = sdf_rounded_rect(in.uv, center, half_size, params.corner_radius);
    let fill_alpha = 1.0 - smoothstep(-0.5, 0.5, dist);
    let fill = params.color * vec4<f32>(1.0, 1.0, 1.0, fill_alpha);

    // Border
    let border_dist = abs(dist) - params.border_width * 0.5;
    let border_alpha = 1.0 - smoothstep(-0.5, 0.5, border_dist);
    let border = params.border_color * vec4<f32>(1.0, 1.0, 1.0, border_alpha * fill_alpha);

    // Composite: shadow behind, fill on top, border on top of fill
    var color = shadow;
    color = mix(color, fill, fill.a);
    color = mix(color, border, border.a);
    return color;
}
```

- [ ] **Step 2: Create WidgetPipeline struct and Rust-side types**

In `src/renderer/pipelines.rs`:

```rust
use bytemuck::{Pod, Zeroable};

/// CPU-side mirror of the WGSL WidgetParams uniform.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct WidgetParams {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub border_color: [f32; 4],
    pub corner_radius: f32,
    pub border_width: f32,
    pub shadow_offset: [f32; 2],
    pub shadow_blur: f32,
    pub shadow_color: [f32; 4],
    pub viewport_size: [f32; 2],
    pub _padding: [f32; 2],
}

pub struct WidgetPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
}

impl WidgetPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sdf_widget_shader"),
            source: wgpu::ShaderSource::Wgsl(SDF_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("widget_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("widget_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("widget_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[], // no vertex buffer, generated in shader
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("widget_uniform"),
            size: std::mem::size_of::<WidgetParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self { pipeline, bind_group_layout, uniform_buffer }
    }

    /// Draw a single widget. Call within an active render pass.
    pub fn draw_widget<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &WidgetParams,
    ) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(params));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("widget_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..4, 0..1); // 4 vertices for triangle strip quad
    }
}
```

Note: The `draw_widget` method creates a new bind group per call. This is fine for MVP (< 20 widgets per frame). For optimization later, pre-allocate bind groups or use dynamic offsets.

- [ ] **Step 3: Integrate into Renderer**

Update `src/renderer/mod.rs`:
- Add `WidgetPipeline` as a field
- Initialize in `new()`
- In the render method (after clear, before text), draw test widgets: a panel-shaped rounded rect with shadow to verify the pipeline works

Test params for a floating panel:

```rust
let params = WidgetParams {
    rect: [20.0, 20.0, 220.0, 400.0],
    color: [0.12, 0.12, 0.14, 0.85],
    border_color: [0.3, 0.3, 0.35, 0.5],
    corner_radius: 12.0,
    border_width: 1.0,
    shadow_offset: [4.0, 4.0],
    shadow_blur: 16.0,
    shadow_color: [0.0, 0.0, 0.0, 0.4],
    viewport_size: [width as f32, height as f32],
    _padding: [0.0, 0.0],
};
```

- [ ] **Step 4: Verify it runs**

Run: `cargo run`
Expected: a dark rounded rectangle with subtle border and drop shadow visible on screen, demonstrating the SDF pipeline works.

- [ ] **Step 5: Commit**

```bash
git add src/renderer/pipelines.rs src/renderer/mod.rs
git commit -m "Add SDF widget pipeline with rounded rects, borders, and shadows"
```

- [ ] **Step 6: Add backdrop blur pipeline**

The blur pipeline is a separate render technique used for panel backgrounds. Implementation approach:

1. Before rendering panels, copy the current framebuffer region behind each panel to a temporary texture
2. Downsample the region with a two-pass (horizontal + vertical) Gaussian blur shader
3. Composite the blurred texture under the panel fill

This requires:
- A second WGSL shader for Gaussian blur (separable, two passes)
- A `BlurPipeline` struct with two render passes (horizontal, vertical)
- A scratch texture at half-resolution for the downsample
- Integration into the render loop: blur runs between preview and widget passes

**Blur shader (horizontal pass):**

```wgsl
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(0) @binding(2) var<uniform> direction: vec2<f32>; // (1/width, 0) or (0, 1/height)

@fragment
fn fs_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var color = textureSample(input_tex, tex_sampler, in.uv) * weights[0];
    for (var i = 1; i < 5; i++) {
        let offset = direction * f32(i);
        color += textureSample(input_tex, tex_sampler, in.uv + offset) * weights[i];
        color += textureSample(input_tex, tex_sampler, in.uv - offset) * weights[i];
    }
    return color;
}
```

This can be deferred to a polish pass if it blocks progress — panels render fine with solid semi-transparent backgrounds. Add backdrop blur as a visual enhancement once the core pipeline is working.

- [ ] **Step 7: Commit blur pipeline (if implemented)**

```bash
git add src/renderer/pipelines.rs src/renderer/mod.rs
git commit -m "Add backdrop blur pipeline for panel transparency effect"
```

---

### Task 10: Preview Texture Stub

**Files:**
- Modify: `src/renderer/preview.rs`
- Modify: `src/renderer/mod.rs`

- [ ] **Step 1: Implement PreviewRenderer**

Write `src/renderer/preview.rs`:

```rust
pub struct PreviewRenderer {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
}
```

Methods:
- `new(device, format, width, height)` — creates a texture and a fullscreen quad pipeline with a simple texture sampling shader
- `upload_frame(queue, frame: &RgbaFrame)` — writes RGBA data to the texture via `queue.write_texture()`
- `render(render_pass)` — draws the fullscreen quad with the texture

The preview renders behind all UI elements (first render pass after clear).

- [ ] **Step 2: Integrate into Renderer**

Update `src/renderer/mod.rs`:
- Add `PreviewRenderer` as a field
- In the render method, call `preview.render()` before widget and text passes
- Upload a test frame (solid dark gray from `MockObsEngine::get_frame()`) on init

- [ ] **Step 3: Verify it runs**

Run: `cargo run`
Expected: window shows a fullscreen dark gray texture (the mock frame) behind the egui test panel and any test widgets.

- [ ] **Step 4: Commit**

```bash
git add src/renderer/preview.rs src/renderer/mod.rs
git commit -m "Add preview texture pipeline with fullscreen quad rendering"
```

---

### Task 11: Scene Editor Panel

**Files:**
- Modify: `src/ui/scene_editor.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement scene editor layout**

Write `src/ui/scene_editor.rs`:

Build the left-edge panel using egui layout:
- Header: "Scenes" with add/remove buttons
- Scene list: selectable items, clicking sets active scene
- Divider
- Header: "Sources" with add/remove buttons
- Source list: selectable items showing name and type
- For selected source: position (x, y) and size (width, height) controls as drag values

The panel uses `egui::SidePanel::left()` with a fixed width (~220px).

All interactions mutate `AppState` (passed as `&mut`).

- [ ] **Step 2: Wire into UiRoot**

Update `src/ui/mod.rs`:
- Remove the test window
- Call `scene_editor::draw(ctx, state)` in the `run` method

- [ ] **Step 3: Verify it runs**

Run: `cargo run`
Expected: left panel shows "Scenes" with "Scene 1" listed, "Sources" section below. Can click to select scenes.

- [ ] **Step 4: Commit**

```bash
git add src/ui/scene_editor.rs src/ui/mod.rs
git commit -m "Add scene editor panel with scene/source list and transform controls"
```

---

### Task 12: Audio Mixer Panel

**Files:**
- Modify: `src/ui/audio_mixer.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement audio mixer layout**

Write `src/ui/audio_mixer.rs`:

Build the bottom-bar panel using `egui::TopBottomPanel::bottom()`:
- For each source with audio: vertical column containing:
  - Source name label (top)
  - Vertical slider (fader) for volume (0.0 to 1.0)
  - VU meter bar next to the fader (reads from `AppState.audio_levels`)
  - Mute toggle button (bottom)
- Columns arranged horizontally

VU meter is a custom egui widget: a vertical rect filled proportionally to the dB level, with a peak hold indicator line.

- [ ] **Step 2: Wire into UiRoot**

Update `src/ui/mod.rs`:
- Call `audio_mixer::draw(ctx, state)` in the `run` method

- [ ] **Step 3: Verify it runs**

Run: `cargo run`
Expected: bottom panel shows mixer strips. If no sources exist yet, shows empty mixer.

- [ ] **Step 4: Commit**

```bash
git add src/ui/audio_mixer.rs src/ui/mod.rs
git commit -m "Add audio mixer panel with faders, VU meters, and mute toggles"
```

---

### Task 13: Stream Controls Panel

**Files:**
- Modify: `src/ui/stream_controls.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Implement stream controls layout**

Write `src/ui/stream_controls.rs`:

Build the top-right floating panel using `egui::Window` positioned at top-right:
- **Go Live / Stop button:** large, prominent. Text changes based on `StreamStatus`. Red when live.
- **Destination selector:** dropdown/combo box for Twitch/YouTube/Custom RTMP
- **Stream key input:** password-style text field
- **Live stats** (visible only when `StreamStatus::Live`):
  - Bitrate: e.g., "4500 kbps"
  - Dropped frames: count
  - Uptime: formatted as HH:MM:SS

Button clicks call `MockObsEngine` methods through `AppState` (or directly — wiring to be refined).

- [ ] **Step 2: Wire into UiRoot**

Update `src/ui/mod.rs`:
- Call `stream_controls::draw(ctx, state)` in the `run` method

- [ ] **Step 3: Verify it runs**

Run: `cargo run`
Expected: top-right floating panel with Go Live button, destination dropdown, and stream key field. Clicking Go Live changes status to Live and shows stats (with mock values).

- [ ] **Step 4: Commit**

```bash
git add src/ui/stream_controls.rs src/ui/mod.rs
git commit -m "Add stream controls panel with go-live, destination, and stats display"
```

---

### Task 14: Mock Data Driver

**Files:**
- Create: `src/mock_driver.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write test for mock data generation**

Test in `src/mock_driver.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_level_random_walk_stays_in_range() {
        let mut level = -30.0_f32;
        for _ in 0..1000 {
            level = random_walk_db(level);
            assert!(level >= -60.0);
            assert!(level <= 0.0);
        }
    }

    #[test]
    fn peak_decay_reduces_over_time() {
        let peak = decay_peak(-5.0, 1.0 / 30.0);
        assert!(peak < -5.0);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib mock_driver`
Expected: FAIL

- [ ] **Step 3: Implement mock data driver**

Write `src/mock_driver.rs`:

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use tokio::time;

use crate::obs::SourceId;
use crate::state::{AppState, AudioLevel, StreamStatus};

/// Random walk for audio level simulation. Stays within -60..0 dB range.
pub fn random_walk_db(current: f32) -> f32 {
    let mut rng = rand::rng();
    let delta: f32 = rng.random_range(-3.0..3.0);
    (current + delta).clamp(-60.0, 0.0)
}

/// Decay peak hold toward current level.
pub fn decay_peak(peak: f32, dt: f32) -> f32 {
    peak - (20.0 * dt) // decay at 20 dB/s
}

/// Spawn a tokio task that updates AppState with mock audio/stream data at ~30Hz.
pub fn spawn_mock_driver(state: Arc<Mutex<AppState>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33)); // ~30Hz
        let mut levels: Vec<f32> = Vec::new();
        let mut peaks: Vec<f32> = Vec::new();
        let mut uptime: f64 = 0.0;

        loop {
            interval.tick().await;
            let dt = 1.0 / 30.0;

            let mut state = state.lock().expect("lock state");

            // Ensure we have enough level entries for all sources
            while levels.len() < state.sources.len() {
                levels.push(-30.0);
                peaks.push(-60.0);
            }

            // Update audio levels
            state.audio_levels.clear();
            for (i, source) in state.sources.iter().enumerate() {
                if i < levels.len() {
                    levels[i] = random_walk_db(levels[i]);
                    if levels[i] > peaks[i] {
                        peaks[i] = levels[i];
                    } else {
                        peaks[i] = decay_peak(peaks[i], dt).max(levels[i]);
                    }

                    state.audio_levels.push(AudioLevel {
                        source_id: source.id,
                        current_db: levels[i],
                        peak_db: peaks[i],
                    });
                }
            }

            // Update stream stats if live
            if let StreamStatus::Live {
                ref mut uptime_secs,
                ref mut bitrate_kbps,
                ref mut dropped_frames,
            } = state.stream_status
            {
                uptime += dt as f64;
                *uptime_secs = uptime;

                let mut rng = rand::rng();
                let jitter: f64 = rng.random_range(-100.0..100.0);
                *bitrate_kbps = (4500.0 + jitter).max(0.0);

                if rng.random_range(0..300) == 0 {
                    *dropped_frames += 1;
                }
            } else {
                uptime = 0.0;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_level_random_walk_stays_in_range() {
        let mut level = -30.0_f32;
        for _ in 0..1000 {
            level = random_walk_db(level);
            assert!(level >= -60.0);
            assert!(level <= 0.0);
        }
    }

    #[test]
    fn peak_decay_reduces_over_time() {
        let peak = decay_peak(-5.0, 1.0 / 30.0);
        assert!(peak < -5.0);
    }
}
```

- [ ] **Step 4: Wire into main.rs**

Update `src/main.rs`:
- Add `mod mock_driver;`
- After renderer init, spawn a tokio runtime and start the mock driver
- Pass the `Arc<Mutex<AppState>>` to the driver

Since `winit`'s event loop is not async, use `tokio::runtime::Runtime` created before the event loop, and spawn the driver on it.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib mock_driver`
Expected: all tests pass

- [ ] **Step 6: Verify visually**

Run: `cargo run`
Expected: VU meters in the audio mixer animate with random-walk levels. If streaming is started, stats tick up.

- [ ] **Step 7: Commit**

```bash
git add src/mock_driver.rs src/main.rs
git commit -m "Add mock data driver for animated VU meters and stream stats"
```

---

### Task 15: Panel Collapse & Polish

**Files:**
- Modify: `src/ui/scene_editor.rs`
- Modify: `src/ui/audio_mixer.rs`
- Modify: `src/ui/stream_controls.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Add collapse/expand toggle to each panel**

Each panel checks `UiState` before rendering its full content:
- Scene panel: if `!scene_panel_open`, render only a small icon strip at the left edge
- Audio mixer: if `!mixer_panel_open`, render only a thin bar with VU meters (no faders)
- Stream controls: if `!controls_panel_open`, render only the live indicator dot

Each collapsed state has a clickable icon/button to expand.

- [ ] **Step 2: Add keyboard shortcuts**

In `main.rs` window event handler, handle key events:
- `F1` — toggle scene panel
- `F2` — toggle mixer panel
- `F3` — toggle controls panel
- `Escape` — close settings modal if open

Update `AppState.ui_state` accordingly.

- [ ] **Step 3: Add settings gear icon**

In `src/ui/mod.rs`, add a small gear button in the top-left (or bottom-left of scene panel). Clicking it sets `ui_state.settings_modal_open = true`.

When open, render a centered `egui::Window` modal with:
- Stream key field
- Destination selector
- Resolution width/height
- Bitrate slider
- Active profile name

Save button writes to `AppSettings` and calls `save_to()`.

- [ ] **Step 4: Verify it runs**

Run: `cargo run`
Expected: panels collapse/expand with keyboard shortcuts. Gear icon opens settings modal. Settings persist across restarts.

- [ ] **Step 5: Commit**

```bash
git add src/ui/ src/main.rs
git commit -m "Add panel collapse/expand, keyboard shortcuts, and settings modal"
```

---

### Task 16: Integration Wiring & Smoke Test

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/stream_controls.rs`

- [ ] **Step 1: Wire MockObsEngine into the app**

In `main.rs`:
- Create a `MockObsEngine` at startup
- Populate `AppState` from the engine's initial scenes/sources
- When stream controls UI triggers "Go Live", call `engine.start_stream()`
- When it triggers "Stop", call `engine.stop_stream()`
- Update `AppState.stream_status` accordingly

The engine lives on the main thread for now (it's a mock). When `LiveObsEngine` is added later, it will move to a dedicated thread with channel communication.

- [ ] **Step 2: Wire scene editor actions**

When the scene editor UI triggers:
- "Add Scene" → `engine.create_scene()`, refresh `AppState.scenes`
- "Remove Scene" → `engine.remove_scene()`, refresh
- "Add Source" → `engine.add_source()`, refresh
- Source transform changes → `engine.update_source_transform()`

- [ ] **Step 3: Wire audio mixer actions**

When the mixer UI triggers:
- Volume fader change → `engine.set_volume()`
- Mute toggle → `engine.set_muted()`

- [ ] **Step 4: End-to-end smoke test**

Run: `cargo run`

Verify manually:
1. Window opens with dark preview background
2. Scene panel shows "Scene 1" on the left
3. Can add a new scene, switch between them
4. Can add sources to scenes
5. Audio mixer shows faders for sources with animated VU meters
6. Stream controls panel in top-right
7. Click "Go Live" → status changes to Live, stats appear and tick
8. Click "Stop" → returns to Offline
9. Panels collapse/expand with F1/F2/F3
10. Settings modal opens with gear icon, saves persist

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "Wire MockObsEngine into UI for end-to-end streaming workflow"
```

---

### Task 17: Final Cleanup & Full Test Suite

**Files:**
- All `src/` files

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Fix any warnings.

- [ ] **Step 3: Run formatter**

Run: `cargo fmt`

- [ ] **Step 4: Verify clean build**

Run: `cargo build --release`
Expected: compiles with no errors or warnings

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "Clean up warnings and formatting for MVP milestone"
```
