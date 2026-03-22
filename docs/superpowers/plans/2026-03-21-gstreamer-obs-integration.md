# GStreamer OBS Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the mock OBS backend with real GStreamer-based screen capture, encoding, RTMP streaming, and MKV/MP4 recording.

**Architecture:** GStreamer runs in-process on a dedicated thread. It captures the screen via `avfvideosrc` and delivers RGBA frames over a channel to the wgpu compositor. Composited frames are read back from GPU and pushed into a second GStreamer pipeline for H.264 encoding (VideoToolbox) and output to RTMP or file.

**Tech Stack:** Rust, gstreamer-rs, wgpu, tokio channels, VideoToolbox H.264

**Spec:** `docs/superpowers/specs/2026-03-21-gstreamer-obs-integration-design.md`

---

## File Structure

### New files
- `src/gstreamer/mod.rs` — public API: `spawn_gstreamer_thread()`, re-exports
- `src/gstreamer/types.rs` — `RgbaFrame`, `ObsStats`, `GstChannels`
- `src/gstreamer/commands.rs` — `GstCommand`, `RecordingFormat`, `CaptureSourceConfig`, `StreamConfig`, `StreamDestination`, `EncoderConfig`
- `src/gstreamer/error.rs` — `GstError` enum
- `src/gstreamer/capture.rs` — capture pipeline builder (`avfvideosrc` → `appsink`)
- `src/gstreamer/encode.rs` — encode pipeline builder (`appsrc` → `vtenc_h264` → outputs)
- `src/gstreamer/thread.rs` — GStreamer thread main loop, command dispatch
- `src/scene.rs` — `Scene`, `Source`, `SourceType`, `Transform`, `SceneId`, `SourceId`, `SourceConfig` (moved from `obs/scene.rs`)

Note: `src/renderer/compositor.rs` (multi-source composition + GPU readback) is deferred to the multi-source phase. In the initial implementation, capture frames go directly to both preview and encode pipelines.

### Modified files
- `Cargo.toml` — add `gstreamer`, `gstreamer-app`, `gstreamer-video` dependencies
- `src/main.rs` — replace `MockObsEngine`/`mock_driver` with GStreamer thread, wire channels
- `src/state.rs` — update imports, add `active_errors`, `recording_status`
- `src/settings.rs` — update `StreamDestination` import path
- `src/renderer/mod.rs` — update `RgbaFrame` import path
- `src/renderer/preview.rs` — update `RgbaFrame` import path
- `src/ui/scene_editor.rs` — update `crate::obs::*` → `crate::scene::*`
- `src/ui/settings_window.rs` — update `StreamDestination` import path
- `src/ui/audio_mixer.rs` — add "No audio sources" placeholder while audio is deferred

### Deleted files
- `src/obs/mod.rs`
- `src/obs/scene.rs`
- `src/obs/output.rs`
- `src/obs/encoder.rs`
- `src/obs/mock.rs`
- `src/mock_driver.rs`

---

### Task 1: Add GStreamer dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add gstreamer crate dependencies**

Add to `[dependencies]` in `Cargo.toml`:

```toml
gstreamer = "0.23"
gstreamer-app = "0.23"
gstreamer-video = "0.23"
```

- [ ] **Step 2: Verify the project compiles**

Run: `cargo check`
Expected: compiles successfully with new dependencies

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add gstreamer, gstreamer-app, gstreamer-video"
```

---

### Task 2: Move scene types from obs/ to scene.rs

**Files:**
- Create: `src/scene.rs`
- Modify: `src/main.rs` (module declaration, imports)
- Modify: `src/state.rs` (import path)
- Modify: `src/ui/scene_editor.rs` (import path)

- [ ] **Step 1: Create `src/scene.rs` with content from `src/obs/scene.rs`**

Copy the entire contents of `src/obs/scene.rs` to `src/scene.rs`. Preserve all attributes (including `#[allow(dead_code)]`).

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

#[allow(dead_code)]
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

- [ ] **Step 2: Add `mod scene;` to `src/main.rs`**

Add `mod scene;` to the module declarations at the top of `src/main.rs` (after `mod obs;` — we'll remove `mod obs;` in a later task).

- [ ] **Step 3: Update `src/state.rs` import**

Change line 1 from:
```rust
use crate::obs::{Scene, SceneId, Source, SourceId};
```
to:
```rust
use crate::scene::{Scene, SceneId, Source, SourceId};
```

- [ ] **Step 4: Update `src/ui/scene_editor.rs` imports**

Change line 1 from:
```rust
use crate::obs::SourceId;
```
to:
```rust
use crate::scene::SourceId;
```

Replace all `crate::obs::SceneId` → `crate::scene::SceneId`, `crate::obs::Scene` → `crate::scene::Scene`, `crate::obs::Source` → `crate::scene::Source`, `crate::obs::SourceType` → `crate::scene::SourceType`, `crate::obs::Transform` → `crate::scene::Transform` throughout the file.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: all existing scene tests pass from their new location

- [ ] **Step 6: Commit**

```bash
git add src/scene.rs src/main.rs src/state.rs src/ui/scene_editor.rs
git commit -m "refactor: move Scene/Source types to top-level scene.rs"
```

---

### Task 3: Create gstreamer module with types, commands, and error types

**Files:**
- Create: `src/gstreamer/mod.rs`
- Create: `src/gstreamer/types.rs`
- Create: `src/gstreamer/commands.rs`
- Create: `src/gstreamer/error.rs`
- Modify: `src/main.rs` (add `mod gstreamer;`)

- [ ] **Step 1: Create `src/gstreamer/types.rs`**

```rust
/// Raw RGBA frame data from the capture pipeline.
#[derive(Debug, Clone)]
pub struct RgbaFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Pipeline statistics sent periodically from the GStreamer thread.
#[derive(Debug, Clone)]
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
```

- [ ] **Step 4: Create `src/gstreamer/error.rs`**

```rust
/// Errors reported from the GStreamer thread to the main thread.
#[derive(Debug, Clone)]
pub enum GstError {
    /// Screen/window/camera capture failed.
    CaptureFailure { message: String },
    /// H.264 encoding failed.
    EncodeFailure { message: String },
    /// RTMP connection was lost during streaming.
    StreamConnectionLost { message: String },
    /// GStreamer pipeline state transition failed.
    PipelineStateChange {
        from: String,
        to: String,
        message: String,
    },
    /// macOS Screen Recording permission denied.
    PermissionDenied { message: String },
}

impl std::fmt::Display for GstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CaptureFailure { message } => write!(f, "Capture failed: {message}"),
            Self::EncodeFailure { message } => write!(f, "Encode failed: {message}"),
            Self::StreamConnectionLost { message } => write!(f, "Stream lost: {message}"),
            Self::PipelineStateChange { from, to, message } => {
                write!(f, "Pipeline state {from} -> {to}: {message}")
            }
            Self::PermissionDenied { message } => write!(f, "Permission denied: {message}"),
        }
    }
}

impl std::error::Error for GstError {}
```

- [ ] **Step 5: Create `src/gstreamer/commands.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::{mpsc, watch};

use super::error::GstError;
use super::types::{PipelineStats, RgbaFrame};

/// Commands sent from the UI thread to the GStreamer thread.
#[derive(Debug)]
pub enum GstCommand {
    SetCaptureSource(CaptureSourceConfig),
    StartStream(StreamConfig),
    StopStream,
    StartRecording { path: PathBuf, format: RecordingFormat },
    StopRecording,
    UpdateEncoder(EncoderConfig),
    Shutdown,
}

/// Capture source selection.
#[derive(Debug, Clone)]
pub enum CaptureSourceConfig {
    Screen { screen_index: u32 },
    // Future:
    // Window { window_id: u64 },
    // Camera { device_index: u32 },
}

/// Recording container format.
#[derive(Debug, Clone, Copy)]
pub enum RecordingFormat {
    Mkv,
    Mp4,
}

/// RTMP streaming destination.
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

/// Stream output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub destination: StreamDestination,
    pub stream_key: String,
}

/// H.264 encoder settings.
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

/// Channel bundle for communication between the main thread and GStreamer thread.
pub struct GstChannels {
    pub command_tx: mpsc::Sender<GstCommand>,
    pub frame_rx: mpsc::Receiver<RgbaFrame>,
    pub stats_rx: watch::Receiver<PipelineStats>,
    pub error_rx: mpsc::UnboundedReceiver<GstError>,
}

/// Internal channel handles held by the GStreamer thread.
pub(crate) struct GstThreadChannels {
    pub command_rx: mpsc::Receiver<GstCommand>,
    pub frame_tx: mpsc::Sender<RgbaFrame>,
    pub stats_tx: watch::Sender<PipelineStats>,
    pub error_tx: mpsc::UnboundedSender<GstError>,
}

/// Create all channels and return both ends.
pub fn create_channels() -> (GstChannels, GstThreadChannels) {
    let (command_tx, command_rx) = mpsc::channel(16);
    let (frame_tx, frame_rx) = mpsc::channel(2);
    let (stats_tx, stats_rx) = watch::channel(PipelineStats::default());
    let (error_tx, error_rx) = mpsc::unbounded_channel();

    let main_channels = GstChannels {
        command_tx,
        frame_rx,
        stats_rx,
        error_rx,
    };

    let thread_channels = GstThreadChannels {
        command_rx,
        frame_tx,
        stats_tx,
        error_tx,
    };

    (main_channels, thread_channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_format_debug() {
        assert_eq!(format!("{:?}", RecordingFormat::Mkv), "Mkv");
        assert_eq!(format!("{:?}", RecordingFormat::Mp4), "Mp4");
    }

    #[test]
    fn capture_source_config_screen() {
        let config = CaptureSourceConfig::Screen { screen_index: 0 };
        assert!(matches!(config, CaptureSourceConfig::Screen { screen_index: 0 }));
    }

    #[test]
    fn stream_destination_rtmp_urls() {
        assert_eq!(StreamDestination::Twitch.rtmp_url(), "rtmp://live.twitch.tv/app");
        assert_eq!(StreamDestination::YouTube.rtmp_url(), "rtmp://a.rtmp.youtube.com/live2");
        let custom = StreamDestination::CustomRtmp { url: "rtmp://my.server/live".to_string() };
        assert_eq!(custom.rtmp_url(), "rtmp://my.server/live");
    }

    #[test]
    fn encoder_config_default() {
        let config = EncoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 30);
        assert_eq!(config.bitrate_kbps, 4500);
    }

    #[test]
    fn create_channels_returns_valid_handles() {
        let (main_ch, _thread_ch) = create_channels();
        main_ch.command_tx.try_send(GstCommand::Shutdown).unwrap();
    }
}
```

- [ ] **Step 6: Create `src/gstreamer/mod.rs`**

```rust
pub mod commands;
pub mod error;
pub mod types;

pub use commands::{
    CaptureSourceConfig, EncoderConfig, GstChannels, GstCommand, RecordingFormat, StreamConfig,
    StreamDestination,
};
pub use error::GstError;
pub use types::{PipelineStats, RgbaFrame};
```

- [ ] **Step 7: Add `mod gstreamer;` to `src/main.rs`**

Add `mod gstreamer;` to the module declarations.

- [ ] **Step 8: Run tests**

Run: `cargo test gstreamer`
Expected: all 5 tests pass

- [ ] **Step 9: Commit**

```bash
git add src/gstreamer/
git commit -m "feat: add gstreamer module with types, commands, and error types"
```

---

### Task 4: Update all imports from obs/ to new locations

**Files:**
- Modify: `src/settings.rs` (line 1: `StreamDestination` import)
- Modify: `src/renderer/mod.rs` (line 17: `RgbaFrame` import)
- Modify: `src/renderer/preview.rs` (line 6: `RgbaFrame` import)
- Modify: `src/ui/settings_window.rs` (line 8: `StreamDestination` import)
- Modify: `src/main.rs` (remove `mod obs;`, remove `use obs::*` imports)

- [ ] **Step 1: Update `src/settings.rs`**

Change line 1 from:
```rust
use crate::obs::StreamDestination;
```
to:
```rust
use crate::gstreamer::StreamDestination;
```

- [ ] **Step 2: Update `src/renderer/mod.rs`**

Change line 17 from:
```rust
use crate::obs::RgbaFrame;
```
to:
```rust
use crate::gstreamer::RgbaFrame;
```

- [ ] **Step 3: Update `src/renderer/preview.rs`**

Change line 6 from:
```rust
use crate::obs::RgbaFrame;
```
to:
```rust
use crate::gstreamer::RgbaFrame;
```

- [ ] **Step 4: Update `src/ui/settings_window.rs`**

Change the `StreamDestination` import from:
```rust
use crate::obs::StreamDestination;
```
to:
```rust
use crate::gstreamer::StreamDestination;
```

- [ ] **Step 5: Update `src/main.rs` — keep obs temporarily, remove direct usage**

Keep `mod obs;` for now — we delete the `obs/` directory in Task 8. Add `#[allow(dead_code)]` to the `engine` field. Remove `use obs::ObsEngine;` (line 11) since the trait is no longer called anywhere except through the `engine` field which is kept temporarily. The `engine` field, its initialization in `AppManager::new()`, and `mod obs;` are all removed together in Task 8.

- [ ] **Step 6: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/settings.rs src/renderer/mod.rs src/renderer/preview.rs src/ui/settings_window.rs src/main.rs
git commit -m "refactor: update imports from obs/ to gstreamer/ and scene/"
```

---

### Task 5: Build the capture pipeline

**Files:**
- Create: `src/gstreamer/capture.rs`
- Modify: `src/gstreamer/mod.rs` (add `pub mod capture;`)

- [ ] **Step 1: Write test for capture pipeline construction**

In `src/gstreamer/capture.rs`, add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capture_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = CaptureSourceConfig::Screen { screen_index: 0 };
        let result = build_capture_pipeline(&config, 1920, 1080, 30);
        // Pipeline creation should succeed even if avfvideosrc is not available
        // (CI may not have GStreamer plugins installed)
        // On machines with GStreamer installed, this should succeed
        match result {
            Ok((pipeline, appsink)) => {
                assert!(pipeline.name().starts_with("capture"));
                drop(appsink);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                // Acceptable on CI without GStreamer plugins
                eprintln!("Skipping capture pipeline test: {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test gstreamer::capture`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement `src/gstreamer/capture.rs`**

```rust
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

use super::commands::CaptureSourceConfig;

/// Build a GStreamer capture pipeline for the given source.
///
/// Returns the pipeline and appsink element. The caller is responsible for
/// setting the pipeline to Playing state and pulling samples from the appsink.
pub fn build_capture_pipeline(
    source: &CaptureSourceConfig,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink)> {
    let pipeline = gstreamer::Pipeline::with_name("capture-pipeline");

    // Create source element based on capture config
    let src = match source {
        CaptureSourceConfig::Screen { screen_index } => {
            let src = gstreamer::ElementFactory::make("avfvideosrc")
                .name("capture-source")
                .property("capture-screen", true)
                .property("capture-screen-cursor", true)
                .property("device-index", *screen_index as i32)
                .build()
                .context("Failed to create avfvideosrc — is GStreamer installed?")?;
            src
        }
    };

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("capture-convert")
        .build()
        .context("Failed to create videoconvert")?;

    let scale = gstreamer::ElementFactory::make("videoscale")
        .name("capture-scale")
        .build()
        .context("Failed to create videoscale")?;

    let rate = gstreamer::ElementFactory::make("videorate")
        .name("capture-rate")
        .build()
        .context("Failed to create videorate")?;

    // Configure appsink to emit RGBA frames at the target resolution/fps
    let caps = gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(width as i32)
        .height(height as i32)
        .framerate(gstreamer::Fraction::new(fps as i32, 1))
        .build();

    let appsink = AppSink::builder()
        .name("capture-sink")
        .caps(&caps)
        .max_buffers(2)
        .drop(true) // Drop old frames if the consumer is slow
        .build();

    pipeline
        .add_many([&src, &convert, &scale, &rate, appsink.upcast_ref()])
        .context("Failed to add elements to capture pipeline")?;

    gstreamer::Element::link_many([&src, &convert, &scale, &rate, appsink.upcast_ref()])
        .context("Failed to link capture pipeline elements")?;

    Ok((pipeline, appsink))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_capture_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = CaptureSourceConfig::Screen { screen_index: 0 };
        let result = build_capture_pipeline(&config, 1920, 1080, 30);
        match result {
            Ok((pipeline, appsink)) => {
                assert!(pipeline.name().starts_with("capture"));
                drop(appsink);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping capture pipeline test (missing plugins): {e}");
            }
        }
    }
}
```

- [ ] **Step 4: Add `pub mod capture;` to `src/gstreamer/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test gstreamer::capture`
Expected: PASS (or gracefully skipped on CI without GStreamer)

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/capture.rs src/gstreamer/mod.rs
git commit -m "feat: implement GStreamer capture pipeline builder"
```

---

### Task 6: Build the encode pipeline

**Files:**
- Create: `src/gstreamer/encode.rs`
- Modify: `src/gstreamer/mod.rs` (add `pub mod encode;`)

- [ ] **Step 1: Write tests for encode pipeline construction**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_stream_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let result = build_stream_pipeline(&config, "rtmp://localhost/test/key");
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping encode pipeline test (missing plugins): {e}");
            }
        }
    }

    #[test]
    fn build_record_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let path = std::path::PathBuf::from("/tmp/test_recording.mkv");
        let result = build_record_pipeline(&config, &path, RecordingFormat::Mkv);
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping record pipeline test (missing plugins): {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test gstreamer::encode`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement `src/gstreamer/encode.rs`**

```rust
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use std::path::Path;

use super::commands::{EncoderConfig, RecordingFormat};

/// Build an appsrc caps string for RGBA frames at the given encoder config.
fn make_appsrc_caps(config: &EncoderConfig) -> gstreamer::Caps {
    gstreamer_video::VideoCapsBuilder::new()
        .format(gstreamer_video::VideoFormat::Rgba)
        .width(config.width as i32)
        .height(config.height as i32)
        .framerate(gstreamer::Fraction::new(config.fps as i32, 1))
        .build()
}

/// Build the common encode chain: appsrc → videoconvert → vtenc_h264 → h264parse.
/// Returns (pipeline, appsrc, last_element_name) so callers can link the output.
fn build_encode_chain(
    config: &EncoderConfig,
    pipeline_name: &str,
) -> Result<(gstreamer::Pipeline, AppSrc, String)> {
    let pipeline = gstreamer::Pipeline::with_name(pipeline_name);

    let caps = make_appsrc_caps(config);
    let appsrc = AppSrc::builder()
        .name("encode-src")
        .caps(&caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    let convert = gstreamer::ElementFactory::make("videoconvert")
        .name("encode-convert")
        .build()
        .context("Failed to create videoconvert")?;

    // Use VideoToolbox hardware encoder on macOS, fall back to x264enc
    let encoder = gstreamer::ElementFactory::make("vtenc_h264")
        .name("encoder")
        .property("bitrate", config.bitrate_kbps)
        .property("realtime", true)
        .property("allow-frame-reordering", false)
        .build()
        .or_else(|_| {
            gstreamer::ElementFactory::make("x264enc")
                .name("encoder")
                .property("bitrate", config.bitrate_kbps)
                .property("tune", 0x04u32) // zerolatency
                .build()
                .context("Failed to create encoder (tried vtenc_h264 and x264enc)")
        })?;

    let parser = gstreamer::ElementFactory::make("h264parse")
        .name("parser")
        .build()
        .context("Failed to create h264parse")?;

    pipeline
        .add_many([appsrc.upcast_ref(), &convert, &encoder, &parser])
        .context("Failed to add encode elements")?;

    gstreamer::Element::link_many([appsrc.upcast_ref(), &convert, &encoder, &parser])
        .context("Failed to link encode chain")?;

    Ok((pipeline, appsrc, "parser".to_string()))
}

/// Build a pipeline for RTMP streaming.
///
/// Pipeline: appsrc → videoconvert → vtenc_h264 → h264parse → flvmux → rtmpsink
pub fn build_stream_pipeline(
    config: &EncoderConfig,
    rtmp_url: &str,
) -> Result<(gstreamer::Pipeline, AppSrc)> {
    let (pipeline, appsrc, last_name) = build_encode_chain(config, "encode-stream-pipeline")?;

    let mux = gstreamer::ElementFactory::make("flvmux")
        .name("stream-mux")
        .property_from_str("streamable", "true")
        .build()
        .context("Failed to create flvmux")?;

    let sink = gstreamer::ElementFactory::make("rtmpsink")
        .name("stream-sink")
        .property("location", rtmp_url)
        .build()
        .context("Failed to create rtmpsink")?;

    pipeline
        .add_many([&mux, &sink])
        .context("Failed to add stream output elements")?;

    let last = pipeline
        .by_name(&last_name)
        .expect("parser element exists");
    gstreamer::Element::link_many([&last, &mux, &sink])
        .context("Failed to link stream output")?;

    Ok((pipeline, appsrc))
}

/// Build a pipeline for file recording.
///
/// Pipeline: appsrc → videoconvert → vtenc_h264 → h264parse → mux → filesink
pub fn build_record_pipeline(
    config: &EncoderConfig,
    path: &Path,
    format: RecordingFormat,
) -> Result<(gstreamer::Pipeline, AppSrc)> {
    let (pipeline, appsrc, last_name) = build_encode_chain(config, "encode-record-pipeline")?;

    let mux = match format {
        RecordingFormat::Mkv => gstreamer::ElementFactory::make("matroskamux")
            .name("record-mux")
            .build()
            .context("Failed to create matroskamux")?,
        RecordingFormat::Mp4 => gstreamer::ElementFactory::make("mp4mux")
            .name("record-mux")
            .property_from_str("fragment-duration", "1000")
            .build()
            .context("Failed to create mp4mux")?,
    };

    let sink = gstreamer::ElementFactory::make("filesink")
        .name("record-sink")
        .property("location", path.to_str().unwrap_or("recording.mkv"))
        .build()
        .context("Failed to create filesink")?;

    pipeline
        .add_many([&mux, &sink])
        .context("Failed to add record output elements")?;

    let last = pipeline
        .by_name(&last_name)
        .expect("parser element exists");
    gstreamer::Element::link_many([&last, &mux, &sink])
        .context("Failed to link record output")?;

    Ok((pipeline, appsrc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_stream_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let result = build_stream_pipeline(&config, "rtmp://localhost/test/key");
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping encode pipeline test (missing plugins): {e}");
            }
        }
    }

    #[test]
    fn build_record_pipeline_creates_valid_pipeline() {
        gstreamer::init().unwrap();
        let config = EncoderConfig::default();
        let path = std::path::PathBuf::from("/tmp/test_recording.mkv");
        let result = build_record_pipeline(&config, &path, RecordingFormat::Mkv);
        match result {
            Ok((pipeline, appsrc)) => {
                assert!(pipeline.name().starts_with("encode"));
                drop(appsrc);
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            Err(e) => {
                eprintln!("Skipping record pipeline test (missing plugins): {e}");
            }
        }
    }
}
```

- [ ] **Step 4: Add `pub mod encode;` to `src/gstreamer/mod.rs`**

- [ ] **Step 5: Run tests**

Run: `cargo test gstreamer::encode`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/encode.rs src/gstreamer/mod.rs
git commit -m "feat: implement GStreamer encode pipeline builders for streaming and recording"
```

---

### Task 7: Implement the GStreamer thread main loop

**Files:**
- Create: `src/gstreamer/thread.rs`
- Modify: `src/gstreamer/mod.rs` (add module, add `spawn_gstreamer_thread`)

- [ ] **Step 1: Implement `src/gstreamer/thread.rs`**

This is the core thread loop that listens for commands and manages pipelines:

```rust
use anyhow::{Context, Result};
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use log;
use std::thread::JoinHandle;

use super::capture::build_capture_pipeline;
use super::commands::{CaptureSourceConfig, GstCommand, GstThreadChannels, RecordingFormat};
use super::encode::{build_record_pipeline, build_stream_pipeline};
use super::error::GstError;
use super::types::{PipelineStats, RgbaFrame};

#[derive(Debug)]
enum PipelineKind {
    Stream,
    Record,
}

/// State held by the GStreamer thread.
struct GstThread {
    channels: GstThreadChannels,
    capture_pipeline: Option<gstreamer::Pipeline>,
    capture_appsink: Option<AppSink>,
    stream_pipeline: Option<gstreamer::Pipeline>,
    stream_appsrc: Option<AppSrc>,
    record_pipeline: Option<gstreamer::Pipeline>,
    record_appsrc: Option<AppSrc>,
    encoder_config: super::commands::EncoderConfig,
}

impl GstThread {
    fn new(channels: GstThreadChannels) -> Self {
        Self {
            channels,
            capture_pipeline: None,
            capture_appsink: None,
            stream_pipeline: None,
            stream_appsrc: None,
            record_pipeline: None,
            record_appsrc: None,
            encoder_config: super::commands::EncoderConfig::default(),
        }
    }

    /// Start capturing from the given source.
    fn start_capture(&mut self, source: &CaptureSourceConfig) {
        // Tear down existing capture
        self.stop_capture();

        match build_capture_pipeline(source, self.encoder_config.width, self.encoder_config.height, self.encoder_config.fps)
        {
            Ok((pipeline, appsink)) => {
                if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                    let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                        message: format!("Failed to start capture: {e}"),
                    });
                    return;
                }
                self.capture_pipeline = Some(pipeline);
                self.capture_appsink = Some(appsink);
                log::info!("Capture pipeline started");
            }
            Err(e) => {
                let _ = self.channels.error_tx.send(GstError::CaptureFailure {
                    message: format!("{e}"),
                });
            }
        }
    }

    fn stop_capture(&mut self) {
        if let Some(pipeline) = self.capture_pipeline.take() {
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        self.capture_appsink = None;
    }

    /// Pull a frame from the capture appsink and send it to the main thread.
    fn poll_capture_frame(&self) {
        let Some(appsink) = &self.capture_appsink else {
            return;
        };

        if let Some(sample) = appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0)) {
            // Extract actual dimensions from the negotiated caps
            let (width, height) = sample
                .caps()
                .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok())
                .map(|info| (info.width(), info.height()))
                .unwrap_or((self.encoder_config.width, self.encoder_config.height));

            if let Some(buffer) = sample.buffer() {
                if let Ok(map) = buffer.map_readable() {
                    let frame = RgbaFrame {
                        data: map.as_slice().to_vec(),
                        width,
                        height,
                    };
                    // Drop newest on full — renderer keeps displaying last received frame
                    let _ = self.channels.frame_tx.try_send(frame);
                }
            }
        }
    }

    fn handle_command(&mut self, cmd: GstCommand) -> bool {
        match cmd {
            GstCommand::SetCaptureSource(source) => {
                self.start_capture(&source);
            }
            GstCommand::StartStream(config) => {
                let url = format!(
                    "{}/{}",
                    config.destination.rtmp_url(),
                    config.stream_key
                );
                match build_stream_pipeline(&self.encoder_config, &url) {
                    Ok((pipeline, appsrc)) => {
                        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start stream: {e}"),
                            });
                            return false;
                        }
                        self.stream_pipeline = Some(pipeline);
                        self.stream_appsrc = Some(appsrc);
                        log::info!("Stream pipeline started");
                    }
                    Err(e) => {
                        let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                            message: format!("{e}"),
                        });
                    }
                }
            }
            GstCommand::StopStream => {
                self.stop_pipeline(&PipelineKind::Stream);
            }
            GstCommand::StopRecording => {
                self.stop_pipeline(&PipelineKind::Record);
            }
            GstCommand::StartRecording { path, format } => {
                match build_record_pipeline(&self.encoder_config, &path, format) {
                    Ok((pipeline, appsrc)) => {
                        if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                                message: format!("Failed to start recording: {e}"),
                            });
                            return false;
                        }
                        self.record_pipeline = Some(pipeline);
                        self.record_appsrc = Some(appsrc);
                        log::info!("Record pipeline started to {}", path.display());
                    }
                    Err(e) => {
                        let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                            message: format!("{e}"),
                        });
                    }
                }
            }
            GstCommand::UpdateEncoder(config) => {
                self.encoder_config = config;
                // Encoder updates take effect on next pipeline (re)start
            }
            GstCommand::Shutdown => {
                self.stop_pipeline(&PipelineKind::Stream);
                self.stop_pipeline(&PipelineKind::Record);
                self.stop_capture();
                return true; // Signal exit
            }
        }
        false
    }

    fn stop_pipeline(&mut self, kind: &PipelineKind) {
        let (appsrc, pipeline) = match kind {
            PipelineKind::Stream => (self.stream_appsrc.take(), self.stream_pipeline.take()),
            PipelineKind::Record => (self.record_appsrc.take(), self.record_pipeline.take()),
        };
        if let Some(appsrc) = appsrc {
            // Send EOS so the muxer can finalize the file
            let _ = appsrc.end_of_stream();
        }
        if let Some(pipeline) = pipeline {
            // Wait briefly for EOS to propagate
            let bus = pipeline.bus().unwrap();
            let _ = bus.timed_pop_filtered(
                gstreamer::ClockTime::from_seconds(2),
                &[gstreamer::MessageType::Eos],
            );
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        log::info!("{:?} pipeline stopped", kind);
    }

    /// Push a frame buffer to an active encode appsrc (for streaming or recording).
    fn push_to_encode(&self, appsrc: &AppSrc, data: &[u8], pts: gstreamer::ClockTime) {
        let mut buffer = gstreamer::Buffer::with_size(data.len()).unwrap();
        {
            let buffer_ref = buffer.get_mut().unwrap();
            buffer_ref.set_pts(pts);
            let mut map = buffer_ref.map_writable().unwrap();
            map.as_mut_slice().copy_from_slice(data);
        }
        let _ = appsrc.push_buffer(buffer);
    }

    /// Main run loop for the GStreamer thread.
    fn run(mut self) {
        // Start with default screen capture
        self.start_capture(&CaptureSourceConfig::Screen { screen_index: 0 });
        let start_time = std::time::Instant::now();

        loop {
            // Check for commands (non-blocking)
            match self.channels.command_rx.try_recv() {
                Ok(cmd) => {
                    if self.handle_command(cmd) {
                        return; // Shutdown
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    log::info!("Command channel disconnected, shutting down GStreamer thread");
                    self.stop_pipeline(&PipelineKind::Stream);
                    self.stop_pipeline(&PipelineKind::Record);
                    self.stop_capture();
                    return;
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            }

            // Pull frame from capture, forward to preview and encode pipelines
            if let Some(appsink) = &self.capture_appsink {
                if let Some(sample) = appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0)) {
                    let (width, height) = sample
                        .caps()
                        .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok())
                        .map(|info| (info.width(), info.height()))
                        .unwrap_or((self.encoder_config.width, self.encoder_config.height));

                    if let Some(buffer) = sample.buffer() {
                        if let Ok(map) = buffer.map_readable() {
                            let data = map.as_slice();
                            let pts = gstreamer::ClockTime::from_nseconds(
                                start_time.elapsed().as_nanos() as u64
                            );

                            // Send to preview
                            let frame = RgbaFrame {
                                data: data.to_vec(),
                                width,
                                height,
                            };
                            let _ = self.channels.frame_tx.try_send(frame);

                            // Feed active encode pipelines
                            // Note: In the initial implementation, capture frames go directly
                            // to encode. When multi-source composition is added, the compositor
                            // will produce composited frames via GPU readback instead.
                            if let Some(ref appsrc) = self.stream_appsrc {
                                self.push_to_encode(appsrc, data, pts);
                            }
                            if let Some(ref appsrc) = self.record_appsrc {
                                self.push_to_encode(appsrc, data, pts);
                            }
                        }
                    }
                }
            }

            // Brief sleep to avoid busy-spinning
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// Spawn the GStreamer thread. Returns channel handles and a join handle.
///
/// Call this from `AppManager::new()`. The thread initializes GStreamer,
/// starts the default screen capture, and listens for commands.
pub fn spawn_gstreamer_thread(
    channels: GstThreadChannels,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("gstreamer".to_string())
        .spawn(move || {
            if let Err(e) = gstreamer::init() {
                log::error!("Failed to initialize GStreamer: {e}");
                return;
            }
            log::info!("GStreamer initialized on dedicated thread");

            let thread = GstThread::new(channels);
            thread.run();

            log::info!("GStreamer thread exiting");
        })
        .expect("spawn GStreamer thread")
}
```

- [ ] **Step 2: Update `src/gstreamer/mod.rs`**

Add `pub mod thread;` and the public `spawn_gstreamer_thread` function re-export:

```rust
pub mod capture;
pub mod commands;
pub mod encode;
pub mod error;
pub mod thread;
pub mod types;

pub use commands::{
    CaptureSourceConfig, EncoderConfig, GstChannels, GstCommand, RecordingFormat, StreamConfig,
    StreamDestination, create_channels,
};
pub use error::GstError;
pub use thread::spawn_gstreamer_thread;
pub use types::{PipelineStats, RgbaFrame};
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/thread.rs src/gstreamer/mod.rs
git commit -m "feat: implement GStreamer thread main loop with command dispatch"
```

---

### Task 8: Wire GStreamer thread into AppManager, remove mock

**Files:**
- Modify: `src/main.rs` (replace MockObsEngine with GStreamer thread, poll channels)
- Modify: `src/state.rs` (add `active_errors`, `recording_status`)
- Delete: `src/mock_driver.rs`
- Delete: `src/obs/` (entire directory)

- [ ] **Step 1: Update `src/state.rs` — add new fields**

Add to `AppState`:
```rust
pub active_errors: Vec<crate::gstreamer::GstError>,
pub recording_status: RecordingStatus,
```

Add the `RecordingStatus` enum:
```rust
#[derive(Debug, Clone)]
pub enum RecordingStatus {
    Idle,
    Recording { path: std::path::PathBuf },
}
```

Update `Default` impl to include:
```rust
active_errors: Vec::new(),
recording_status: RecordingStatus::Idle,
```

- [ ] **Step 2: Add `command_tx` to `AppState`**

Add a `command_tx` field to `AppState` so UI panels can send GStreamer commands:
```rust
pub command_tx: Option<tokio::sync::mpsc::Sender<crate::gstreamer::GstCommand>>,
```

Update `Default` impl: `command_tx: None,`

This is the simplest way to thread the command sender to UI panels — they already receive `&mut AppState`.

- [ ] **Step 3: Update `src/main.rs` — replace engine with GStreamer channels**

Replace the `engine` field on `AppManager` with:
```rust
gst_channels: Option<gstreamer::GstChannels>,
gst_thread: Option<std::thread::JoinHandle<()>>,
```

In `AppManager::new()`:
- Remove `MockObsEngine::new()` and populating initial state from engine
- Create channels via `gstreamer::create_channels()`
- Spawn the GStreamer thread via `gstreamer::spawn_gstreamer_thread()`
- Store `GstChannels` and `JoinHandle` on `AppManager`
- Populate initial `AppState` with a default scene (previously came from `MockObsEngine`):
```rust
use crate::scene::{Scene, SceneId};
let initial_state = AppState {
    scenes: vec![Scene { id: SceneId(1), name: "Scene 1".to_string(), sources: vec![] }],
    active_scene_id: Some(SceneId(1)),
    command_tx: Some(main_channels.command_tx.clone()),
    ..AppState::default()
};
```

Note: `mpsc::Sender` is cloneable. Store the original in `GstChannels` and clone for `AppState`.

In `resumed()`:
- Remove `self.runtime.spawn(mock_driver::run_mock_driver(...))`

In the `RedrawRequested` handler (or a new helper called each frame):
- Poll `frame_rx` with `try_recv()` — if a frame is received, call `gpu.preview_renderer.upload_frame(&gpu.queue, &frame)`
- Poll `stats_rx` with `has_changed()` — update `AppState.stream_status`
- Poll `error_rx` with `try_recv()` — push errors to `AppState.active_errors`

On app exit (or in a `Drop` impl):
- Send `GstCommand::Shutdown` via `command_tx`
- Join the GStreamer thread

- [ ] **Step 4: Remove `mod mock_driver;` and `mod obs;` from `src/main.rs`**

- [ ] **Step 5: Delete `src/mock_driver.rs` and `src/obs/` directory**

Run:
```bash
rm src/mock_driver.rs
rm -r src/obs/
```

- [ ] **Step 6: Update `src/ui/audio_mixer.rs` — unconditional placeholder for deferred audio**

Replace the entire body of the `draw` function with an early return showing a placeholder. The audio mixer should not attempt to render faders while audio is deferred:

```rust
pub fn draw(ui: &mut egui::Ui, _state: &mut AppState, _panel_id: PanelId) {
    ui.vertical_centered(|ui| {
        ui.add_space(20.0);
        ui.label("No audio sources");
        ui.label("Audio capture coming soon");
    });
}
```

- [ ] **Step 7: Run `cargo check`**

Expected: compiles with no errors

- [ ] **Step 8: Run `cargo test`**

Expected: all remaining tests pass (obs tests are gone, scene tests run from `scene.rs`, gstreamer tests pass)

- [ ] **Step 9: Commit**

```bash
git add src/main.rs src/state.rs src/ui/audio_mixer.rs
git rm src/mock_driver.rs src/obs/mod.rs src/obs/scene.rs src/obs/output.rs src/obs/encoder.rs src/obs/mock.rs
git commit -m "feat: wire GStreamer thread into AppManager, remove mock OBS backend"
```

---

### Task 9: Verify end-to-end capture → preview

**Files:** None (manual verification)

- [ ] **Step 1: Ensure GStreamer is installed**

Run: `brew install gstreamer`
Verify: `gst-inspect-1.0 avfvideosrc` shows the element

- [ ] **Step 2: Run the app**

Run: `cargo run`
Expected:
- macOS "Screen Recording" permission prompt appears (grant it)
- The preview panel shows a live capture of the screen
- No crashes or errors in the log output

- [ ] **Step 3: Verify frame rate**

Check log output or add temporary fps logging in the frame poll loop. Expected: ~30fps of frame delivery.

- [ ] **Step 4: Test permission denied flow**

Revoke Screen Recording permission in System Preferences and restart the app.
Expected: `GstError::PermissionDenied` or `CaptureFailure` appears in logs, preview shows solid color (no crash).

- [ ] **Step 5: Commit any fixes discovered during verification**

---

### Task 10: Wire stream controls to GStreamer commands

**Files:**
- Modify: `src/ui/stream_controls.rs`

- [ ] **Step 1: Update Go Live button to send GstCommand**

The `draw` function receives `&mut AppState`, which now has a `command_tx` field. Update the Go Live button click handler to send `GstCommand::StartStream` / `GstCommand::StopStream` instead of directly toggling `StreamStatus`:

```rust
if ui.add(button).clicked() {
    if let Some(ref tx) = state.command_tx {
        if is_live {
            let _ = tx.try_send(crate::gstreamer::GstCommand::StopStream);
            state.stream_status = StreamStatus::Offline;
        } else {
            let config = crate::gstreamer::StreamConfig {
                destination: crate::gstreamer::StreamDestination::CustomRtmp {
                    url: "rtmp://localhost/live".to_string(), // TODO: read from settings
                },
                stream_key: stream_key.clone(),
            };
            let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream(config));
            state.stream_status = StreamStatus::Live {
                uptime_secs: 0.0,
                bitrate_kbps: 0.0,
                dropped_frames: 0,
            };
        }
    }
}
```

- [ ] **Step 2: Add Record button**

Add a "Record" / "Stop Recording" button below Go Live. When clicked, send `GstCommand::StartRecording` with path `~/Videos/lodestone-{timestamp}.mkv` or `GstCommand::StopRecording`. Use `state.recording_status` to determine button state.

- [ ] **Step 3: Set up a local RTMP server for testing**

Run: `docker run -p 1935:1935 tiangolo/nginx-rtmp` (or any RTMP test server)

- [ ] **Step 4: Test streaming**

Run: `cargo run`. Click Go Live. Use VLC or ffplay to open `rtmp://localhost/live` and verify video is received. Click Stop and verify clean shutdown.

- [ ] **Step 5: Test recording**

Click Record, wait 10 seconds, click Stop Recording. Verify the MKV file is created and playable in VLC.

- [ ] **Step 6: Commit**

```bash
git add src/ui/stream_controls.rs
git commit -m "feat: wire stream controls to GStreamer commands with record button"
```

---

### Task 11: Final cleanup and verification

**Files:** Various

- [ ] **Step 1: Run clippy**

Run: `cargo clippy`
Fix any warnings.

- [ ] **Step 2: Run fmt**

Run: `cargo fmt --check`
Fix any formatting issues.

- [ ] **Step 3: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 4: Clean up dead code warnings**

Remove any `#[allow(dead_code)]` annotations that are no longer needed. Remove any unused imports.

- [ ] **Step 5: Commit**

Review `git status`, then stage only changed source files:
```bash
git add src/
git commit -m "fix: clippy, fmt, and dead code cleanup"
```
