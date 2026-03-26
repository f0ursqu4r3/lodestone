# Recording & Streaming Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make recording and streaming production-ready with proper encoder detection, separate stream/record quality, filename templates, and wired-up settings UI.

**Architecture:** Extend existing GStreamer command/channel architecture. Add `EncoderType` enum and encoder detection at startup. Split stream/record settings with independent quality presets. Commands carry full config instead of relying on stale thread state. New recording settings tab in settings window.

**Tech Stack:** Rust, GStreamer (gstreamer-rs 0.23), egui, serde/TOML, tokio channels

**Spec:** `docs/superpowers/specs/2026-03-26-recording-streaming-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/gstreamer/commands.rs` | Modify | `EncoderType`, `QualityPreset`, `AvailableEncoder`, updated `GstCommand` variants, updated `EncoderConfig`, new channel in `create_channels`, remove `StreamConfig`/`UpdateEncoder` |
| `src/gstreamer/encode.rs` | Modify | `make_encoder()` dispatch, update `build_encode_chain` to take `EncoderType` |
| `src/gstreamer/thread.rs` | Modify | Encoder detection at startup, updated `handle_start_stream`/`handle_start_recording`, remove `self.encoder_config` |
| `src/gstreamer/mod.rs` | Modify | Re-export new types |
| `src/settings.rs` | Modify | `RecordSettings` struct, updated `StreamSettings`, `RecordingFormat` serde derives, filename template helpers |
| `src/state.rs` | Modify | Add `available_encoders`, `recording_started_at` to `AppState` |
| `src/ui/toolbar.rs` | Modify | Updated Go Live/Record button logic, recording timer, validation |
| `src/ui/settings/stream.rs` | Modify | Rewrite with encoder dropdown, quality presets, FPS toggles |
| `src/ui/settings/record.rs` | Create | New recording settings tab |
| `src/ui/settings/mod.rs` | Modify | Add `Recording` category, wire up `record.rs` |
| `src/main.rs` | Modify | Wire encoder channel from GStreamer to AppState |

---

### Task 1: Add EncoderType, QualityPreset, and AvailableEncoder types

**Files:**
- Modify: `src/gstreamer/commands.rs:114-165` (after RecordingFormat, before/including EncoderConfig)
- Modify: `src/gstreamer/mod.rs:14-18` (re-exports)

- [ ] **Step 1: Write tests for new types**

Add to the `tests` module at the bottom of `src/gstreamer/commands.rs`:

```rust
#[test]
fn encoder_type_gstreamer_element_name() {
    assert_eq!(EncoderType::H264VideoToolbox.element_name(), "vtenc_h264");
    assert_eq!(EncoderType::H264x264.element_name(), "x264enc");
    assert_eq!(EncoderType::H264Nvenc.element_name(), "nvh264enc");
    assert_eq!(EncoderType::H264Amf.element_name(), "amfh264enc");
    assert_eq!(EncoderType::H264Qsv.element_name(), "qsvh264enc");
}

#[test]
fn encoder_type_display_name() {
    assert_eq!(EncoderType::H264VideoToolbox.display_name(), "VideoToolbox (Hardware)");
    assert_eq!(EncoderType::H264x264.display_name(), "x264 (Software)");
}

#[test]
fn encoder_type_is_hardware() {
    assert!(EncoderType::H264VideoToolbox.is_hardware());
    assert!(!EncoderType::H264x264.is_hardware());
    assert!(EncoderType::H264Nvenc.is_hardware());
}

#[test]
fn quality_preset_to_bitrate() {
    assert_eq!(QualityPreset::Low.bitrate_kbps(), 2500);
    assert_eq!(QualityPreset::Medium.bitrate_kbps(), 4500);
    assert_eq!(QualityPreset::High.bitrate_kbps(), 8000);
}

#[test]
fn quality_preset_custom_returns_none() {
    // Custom preset doesn't have a fixed bitrate — caller provides it
    assert_eq!(QualityPreset::Custom.bitrate_kbps(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test encoder_type_gstreamer quality_preset -- --nocapture`
Expected: compilation errors (types don't exist yet)

- [ ] **Step 3: Implement EncoderType enum**

Add after `RecordingFormat` in `src/gstreamer/commands.rs` (~line 120):

```rust
/// Available H.264 encoder backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncoderType {
    H264VideoToolbox,
    H264x264,
    H264Nvenc,
    H264Amf,
    H264Qsv,
}

impl EncoderType {
    /// GStreamer element factory name for this encoder.
    pub fn element_name(&self) -> &'static str {
        match self {
            Self::H264VideoToolbox => "vtenc_h264",
            Self::H264x264 => "x264enc",
            Self::H264Nvenc => "nvh264enc",
            Self::H264Amf => "amfh264enc",
            Self::H264Qsv => "qsvh264enc",
        }
    }

    /// Human-readable display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::H264VideoToolbox => "VideoToolbox (Hardware)",
            Self::H264x264 => "x264 (Software)",
            Self::H264Nvenc => "NVENC (Hardware)",
            Self::H264Amf => "AMF (Hardware)",
            Self::H264Qsv => "QuickSync (Hardware)",
        }
    }

    /// Whether this is a hardware-accelerated encoder.
    pub fn is_hardware(&self) -> bool {
        !matches!(self, Self::H264x264)
    }

    /// All known encoder types in auto-select priority order.
    pub fn all() -> &'static [EncoderType] {
        &[
            Self::H264VideoToolbox,
            Self::H264Nvenc,
            Self::H264Amf,
            Self::H264Qsv,
            Self::H264x264,
        ]
    }
}
```

- [ ] **Step 4: Implement QualityPreset enum**

Add after `EncoderType` in `src/gstreamer/commands.rs`:

```rust
/// Named quality presets mapping to bitrate values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    Low,
    Medium,
    High,
    Custom,
}

impl QualityPreset {
    /// Bitrate in kbps for this preset. Returns 0 for Custom (caller provides bitrate).
    pub fn bitrate_kbps(&self) -> u32 {
        match self {
            Self::Low => 2500,
            Self::Medium => 4500,
            Self::High => 8000,
            Self::Custom => 0,
        }
    }

    /// All presets in display order.
    pub fn all() -> &'static [QualityPreset] {
        &[Self::Low, Self::Medium, Self::High, Self::Custom]
    }
}
```

- [ ] **Step 5: Implement AvailableEncoder struct**

Add after `QualityPreset`:

```rust
/// An encoder detected as available at startup.
#[derive(Debug, Clone)]
pub struct AvailableEncoder {
    pub encoder_type: EncoderType,
    pub is_recommended: bool,
}
```

- [ ] **Step 6: Add encoder_type field to EncoderConfig**

Update the existing `EncoderConfig` struct (~line 150) to add the field and update the Default impl:

```rust
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_type: EncoderType,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate_kbps: 4500,
            encoder_type: EncoderType::H264VideoToolbox,
        }
    }
}
```

- [ ] **Step 7: Update re-exports in mod.rs**

Update `src/gstreamer/mod.rs` to re-export the new types:

```rust
pub use commands::{
    AudioEncoderConfig, AudioSourceKind, AvailableEncoder, CaptureSourceConfig, EncoderConfig,
    EncoderType, GstChannels, GstCommand, QualityPreset, RecordingFormat, StreamDestination,
    create_channels,
};
```

Note: `StreamConfig` is intentionally omitted — it will be removed in Task 3.

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 9: Commit**

```bash
git add src/gstreamer/commands.rs src/gstreamer/mod.rs
git commit -m "feat: add EncoderType, QualityPreset, and AvailableEncoder types"
```

---

### Task 2: Encoder detection on GStreamer thread

**Files:**
- Modify: `src/gstreamer/commands.rs:167-229` (GstChannels, GstThreadChannels, create_channels)
- Modify: `src/gstreamer/thread.rs:909-922` (run() startup section)
- Modify: `src/gstreamer/thread.rs:47-92` (GstThread struct and new())

- [ ] **Step 1: Write test for encoder detection**

Add to `src/gstreamer/thread.rs` tests module:

```rust
#[test]
fn enumerate_encoders_returns_at_least_one() {
    gstreamer::init().unwrap();
    let encoders = GstThread::enumerate_encoders();
    assert!(!encoders.is_empty(), "should detect at least x264");
    // Exactly one encoder should be recommended
    assert_eq!(
        encoders.iter().filter(|e| e.is_recommended).count(),
        1,
        "exactly one encoder should be recommended"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test enumerate_encoders -- --nocapture`
Expected: compilation error (method doesn't exist)

- [ ] **Step 3: Add encoders channel to GstChannels and GstThreadChannels**

In `src/gstreamer/commands.rs`, add the channel fields to both structs and update `create_channels()`:

Add to `GstChannels` struct:
```rust
pub encoders_rx: tokio::sync::watch::Receiver<Vec<AvailableEncoder>>,
```

Add to `GstThreadChannels` struct:
```rust
pub encoders_tx: tokio::sync::watch::Sender<Vec<AvailableEncoder>>,
```

In `create_channels()`, create the watch channel:
```rust
let (encoders_tx, encoders_rx) = tokio::sync::watch::channel(Vec::new());
```

And add to the returned structs.

- [ ] **Step 4: Implement enumerate_encoders on GstThread**

Add method to `GstThread` impl in `src/gstreamer/thread.rs`:

```rust
/// Probe GStreamer for available H.264 encoders.
fn enumerate_encoders() -> Vec<AvailableEncoder> {
    let mut encoders = Vec::new();
    let mut found_recommended = false;

    for &encoder_type in EncoderType::all() {
        if gstreamer::ElementFactory::make(encoder_type.element_name())
            .build()
            .is_ok()
        {
            let is_hw = encoder_type.is_hardware();
            let is_recommended = !found_recommended && (is_hw || encoder_type == EncoderType::H264x264);
            if is_recommended {
                found_recommended = true;
            }
            encoders.push(AvailableEncoder {
                encoder_type,
                is_recommended,
            });
        }
    }

    // If no encoder was marked recommended yet (shouldn't happen), mark x264
    if !found_recommended {
        if let Some(enc) = encoders.iter_mut().find(|e| e.encoder_type == EncoderType::H264x264) {
            enc.is_recommended = true;
        }
    }

    encoders
}
```

- [ ] **Step 5: Call enumerate_encoders in run() startup**

In the `run()` method, after GStreamer init and before the audio device enumeration, add:

```rust
// Detect available encoders
let encoders = Self::enumerate_encoders();
log::info!("Detected {} encoder(s): {:?}", encoders.len(),
    encoders.iter().map(|e| e.encoder_type.display_name()).collect::<Vec<_>>());
let _ = self.channels.encoders_tx.send(encoders);
```

- [ ] **Step 6: Run tests**

Run: `cargo test enumerate_encoders -- --nocapture`
Expected: PASS

Note: `self.encoder_config` and `self.audio_encoder_config` fields on `GstThread` are left in place for now — they will be removed in Task 4 when the handlers are updated, keeping the codebase compilable between tasks.

- [ ] **Step 7: Commit**

```bash
git add src/gstreamer/commands.rs src/gstreamer/thread.rs
git commit -m "feat: encoder detection at GStreamer startup with watch channel"
```

---

### Task 3: Update GstCommand variants and encode pipeline

**Files:**
- Modify: `src/gstreamer/commands.rs:36-90` (GstCommand enum)
- Modify: `src/gstreamer/encode.rs:43-93` (build_encode_chain)

- [ ] **Step 1: Update GstCommand enum**

In `src/gstreamer/commands.rs`, change the `StartStream` and `StartRecording` variants and remove `UpdateEncoder`:

```rust
pub enum GstCommand {
    StartStream {
        destination: StreamDestination,
        stream_key: String,
        encoder_config: EncoderConfig,
    },
    StopStream,
    StartRecording {
        path: PathBuf,
        format: RecordingFormat,
        encoder_config: EncoderConfig,
    },
    StopRecording,
    // ... rest of variants unchanged
}
```

Remove: `UpdateEncoder(EncoderConfig)` variant and `StreamConfig` struct.

- [ ] **Step 2: Write test for make_encoder dispatch**

Add to `src/gstreamer/encode.rs` tests:

```rust
#[test]
fn make_encoder_creates_element_for_available_type() {
    gstreamer::init().unwrap();
    // x264 should always be available
    let encoder = super::make_encoder(
        crate::gstreamer::EncoderType::H264x264,
        4500,
    );
    assert!(encoder.is_ok(), "x264enc should be available");
}
```

- [ ] **Step 3: Implement make_encoder function**

Add to `src/gstreamer/encode.rs` (before `build_encode_chain`):

```rust
use super::commands::EncoderType;

/// Create a GStreamer H.264 encoder element for the given encoder type.
fn make_encoder(encoder_type: EncoderType, bitrate_kbps: u32) -> Result<gstreamer::Element> {
    match encoder_type {
        EncoderType::H264VideoToolbox => gstreamer::ElementFactory::make("vtenc_h264")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .property("realtime", true)
            .property("allow-frame-reordering", false)
            .build()
            .context("Failed to create vtenc_h264"),
        EncoderType::H264x264 => gstreamer::ElementFactory::make("x264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .property("tune", 0x04u32) // zerolatency
            .build()
            .context("Failed to create x264enc"),
        EncoderType::H264Nvenc => gstreamer::ElementFactory::make("nvh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create nvh264enc"),
        EncoderType::H264Amf => gstreamer::ElementFactory::make("amfh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create amfh264enc"),
        EncoderType::H264Qsv => gstreamer::ElementFactory::make("qsvh264enc")
            .name("encoder")
            .property("bitrate", bitrate_kbps)
            .build()
            .context("Failed to create qsvh264enc"),
    }
}
```

- [ ] **Step 4: Update build_encode_chain to use make_encoder**

Replace the hardcoded encoder creation in `build_encode_chain` (lines 65-78) with:

```rust
let encoder = make_encoder(config.encoder_type, config.bitrate_kbps)?;
```

Remove the old `vtenc_h264` / `x264enc` fallback logic.

- [ ] **Step 5: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass (some thread.rs tests may need updating due to removed fields — fix any compilation errors from the `encoder_config` removal in Task 2 step 6)

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/commands.rs src/gstreamer/encode.rs
git commit -m "feat: encoder dispatch by EncoderType, updated GstCommand variants"
```

---

### Task 4: Update GStreamer thread handlers for new command shapes

**Files:**
- Modify: `src/gstreamer/thread.rs:559-665` (handle_command, handle_start_stream, handle_start_recording)

- [ ] **Step 1: Update handle_command match arms**

Update the `handle_command` method to match new `StartStream`/`StartRecording` shapes:

```rust
GstCommand::StartStream { destination, stream_key, encoder_config } => {
    self.handle_start_stream(destination, stream_key, encoder_config)
}
GstCommand::StartRecording { path, format, encoder_config } => {
    self.handle_start_recording(path, format, encoder_config)
}
```

Remove the `UpdateEncoder` match arm.

- [ ] **Step 2: Update handle_start_stream**

The method now takes individual fields and builds the RTMP URL from destination + key:

```rust
fn handle_start_stream(
    &mut self,
    destination: StreamDestination,
    stream_key: String,
    encoder_config: EncoderConfig,
) {
    let rtmp_url = match &destination {
        StreamDestination::CustomRtmp { url } => url.clone(),
        other => format!("{}/{}", other.rtmp_url(), stream_key),
    };

    let audio_config = AudioEncoderConfig::default();
    match build_stream_pipeline_with_audio(
        &encoder_config,
        &audio_config,
        &rtmp_url,
        self.has_system_audio,
    ) {
        Ok(handles) => {
            if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                    message: format!("Failed to start stream: {e}"),
                });
                return;
            }
            log::info!("Stream pipeline started to {}", destination.rtmp_url());
            self.stream_handles = Some(handles);
        }
        Err(e) => {
            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                message: format!("{e}"),
            });
        }
    }
}
```

- [ ] **Step 3: Update handle_start_recording**

```rust
fn handle_start_recording(
    &mut self,
    path: std::path::PathBuf,
    format: RecordingFormat,
    encoder_config: EncoderConfig,
) {
    let audio_config = AudioEncoderConfig::default();
    match build_record_pipeline_with_audio(
        &encoder_config,
        &audio_config,
        &path,
        format,
        self.has_system_audio,
    ) {
        Ok(handles) => {
            if let Err(e) = handles.pipeline.set_state(gstreamer::State::Playing) {
                let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                    message: format!("Failed to start recording: {e}"),
                });
                return;
            }
            log::info!("Record pipeline started to {}", path.display());
            self.record_handles = Some(handles);
        }
        Err(e) => {
            let _ = self.channels.error_tx.send(GstError::EncodeFailure {
                message: format!("{e}"),
            });
        }
    }
}
```

- [ ] **Step 4: Remove self.encoder_config and self.audio_encoder_config from GstThread**

Remove the `encoder_config: EncoderConfig` and `audio_encoder_config: AudioEncoderConfig` fields from the `GstThread` struct (lines 68-69) and their initialization in `new()` (lines 89-90). Search for any remaining `self.encoder_config` references in `thread.rs` (e.g. in capture pipeline setup) and replace with sensible defaults or remove if unused. The capture pipelines should use their own source-specific resolution, not the encoder config.

- [ ] **Step 5: Update existing tests in thread.rs**

Fix `handle_update_encoder_stores_config` test (remove it — `UpdateEncoder` no longer exists). Update `gst_thread_new_has_defaults` to not check `encoder_config`. Fix any tests that construct `GstCommand::StartStream(StreamConfig { .. })` to use the new struct variant.

- [ ] **Step 6: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "feat: GStreamer thread uses per-command EncoderConfig, removes stale state"
```

---

### Task 5: RecordSettings and updated StreamSettings

**Files:**
- Modify: `src/settings.rs:128-152` (StreamSettings)
- Modify: `src/settings.rs:7-27` (AppSettings)
- Modify: `src/gstreamer/commands.rs` (add Serialize/Deserialize to RecordingFormat)

- [ ] **Step 1: Write tests for filename template expansion**

Add to `src/settings.rs` tests:

```rust
#[test]
fn filename_template_basic_expansion() {
    let result = RecordSettings::expand_template(
        "{date}_{time}_{scene}",
        "Gaming",
        1,
    );
    // Should contain date, time, and scene name
    assert!(result.contains("Gaming"));
    assert!(result.contains('_'));
    // Should not contain template tokens
    assert!(!result.contains('{'));
}

#[test]
fn filename_template_scene_sanitization() {
    let result = RecordSettings::expand_template(
        "{scene}",
        "My Scene/Name",
        1,
    );
    assert_eq!(result, "My_Scene_Name");
}

#[test]
fn filename_template_counter() {
    let r1 = RecordSettings::expand_template("{n}", "scene", 1);
    let r3 = RecordSettings::expand_template("{n}", "scene", 3);
    assert_eq!(r1, "1");
    assert_eq!(r3, "3");
}

#[test]
fn record_settings_default() {
    let settings = RecordSettings::default();
    assert_eq!(settings.format, RecordingFormat::Mkv);
    assert_eq!(settings.filename_template, "{date}_{time}_{scene}");
    assert_eq!(settings.quality_preset, QualityPreset::High);
    assert_eq!(settings.fps, 30);
}

#[test]
fn record_settings_roundtrip() {
    let settings = RecordSettings::default();
    let toml_str = toml::to_string(&settings).unwrap();
    let parsed: RecordSettings = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.format, settings.format);
    assert_eq!(parsed.filename_template, settings.filename_template);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test record_settings filename_template -- --nocapture`
Expected: compilation errors

- [ ] **Step 3: Add Serialize/Deserialize to RecordingFormat**

In `src/gstreamer/commands.rs`, update `RecordingFormat`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingFormat {
    Mkv,
    #[allow(dead_code)]
    Mp4,
}
```

- [ ] **Step 4: Add chrono dependency**

Run: `cargo add chrono`

This is needed for filename template expansion (`{date}`, `{time}` tokens).

- [ ] **Step 5: Implement RecordSettings**

Add to `src/settings.rs` after `StreamSettings`:

```rust
use crate::gstreamer::{EncoderType, QualityPreset, RecordingFormat};

/// Recording output settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordSettings {
    pub format: RecordingFormat,
    pub output_folder: PathBuf,
    pub filename_template: String,
    pub encoder: EncoderType,
    pub quality_preset: QualityPreset,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl Default for RecordSettings {
    fn default() -> Self {
        Self {
            format: RecordingFormat::Mkv,
            output_folder: dirs::video_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from(".")),
            filename_template: "{date}_{time}_{scene}".to_string(),
            encoder: EncoderType::H264VideoToolbox,
            quality_preset: QualityPreset::High,
            bitrate_kbps: 8000,
            fps: 30,
        }
    }
}

impl RecordSettings {
    /// Expand a filename template, replacing tokens with actual values.
    ///
    /// Tokens: `{date}`, `{time}`, `{scene}`, `{n}`
    pub fn expand_template(template: &str, scene_name: &str, counter: u32) -> String {
        let now = chrono::Local::now();
        let sanitized_scene: String = scene_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
            .collect();

        template
            .replace("{date}", &now.format("%Y-%m-%d").to_string())
            .replace("{time}", &now.format("%H-%M-%S").to_string())
            .replace("{scene}", &sanitized_scene)
            .replace("{n}", &counter.to_string())
    }
}
```

- [ ] **Step 6: Update StreamSettings**

Replace the existing `StreamSettings` struct with:

```rust
pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination,
    pub encoder: EncoderType,
    pub quality_preset: QualityPreset,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self {
            stream_key: String::new(),
            destination: StreamDestination::Twitch,
            encoder: EncoderType::H264VideoToolbox,
            quality_preset: QualityPreset::Medium,
            bitrate_kbps: 4500,
            fps: 30,
        }
    }
}
```

Removed: `width`, `height`, `encoder: String`.

- [ ] **Step 7: Add record field to AppSettings**

Add `pub record: RecordSettings` to the `AppSettings` struct.

- [ ] **Step 8: Update existing settings tests**

Fix `stream_settings_roundtrip` and `expanded_settings_roundtrip` to use new field types. Fix `backwards_compat_*` tests if they reference removed fields.

- [ ] **Step 9: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 10: Commit**

```bash
git add src/settings.rs src/gstreamer/commands.rs Cargo.toml
git commit -m "feat: RecordSettings struct, updated StreamSettings with EncoderType/QualityPreset"
```

---

### Task 6: Wire encoder channel and update AppState

**Files:**
- Modify: `src/state.rs:108-169` (AppState struct)
- Modify: `src/main.rs:176-260` (AppManager::new, channel wiring)
- Modify: `src/main.rs:1340-1370` (render loop, encoder channel polling)

- [ ] **Step 1: Add fields to AppState**

In `src/state.rs`, add to `AppState`:

```rust
pub available_encoders: Vec<crate::gstreamer::AvailableEncoder>,
pub recording_started_at: Option<std::time::Instant>,
pub recording_counter: u32,  // auto-incrementing counter for {n} template token
```

Initialize in `Default` or constructor:
```rust
available_encoders: Vec::new(),
recording_started_at: None,
recording_counter: 0,
```

- [ ] **Step 2: Wire encoders channel in main.rs**

In `AppManager::new()`, store the `encoders_rx` from `GstChannels`.

In the main event loop (near where `devices_rx` is polled), add polling for `encoders_rx`:

```rust
// Poll for detected encoders
if let Some(ref channels) = self.gst_channels {
    if channels.encoders_rx.has_changed().unwrap_or(false) {
        let encoders = channels.encoders_rx.borrow().clone();
        let mut app_state = self.state.lock().unwrap();
        app_state.available_encoders = encoders;
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 4: Commit**

```bash
git add src/state.rs src/main.rs
git commit -m "feat: wire encoder detection channel to AppState"
```

---

### Task 7: Update toolbar with validation, recording timer, and new command assembly

**Files:**
- Modify: `src/ui/toolbar.rs` (entire file — stream button, record button, timer)

- [ ] **Step 1: Write validation helper**

Add validation function to `src/ui/toolbar.rs`:

```rust
/// Validate stream settings before starting. Returns error message if invalid.
fn validate_stream_settings(state: &AppState) -> Option<String> {
    match &state.settings.stream.destination {
        crate::gstreamer::StreamDestination::Twitch
        | crate::gstreamer::StreamDestination::YouTube => {
            if state.settings.stream.stream_key.trim().is_empty() {
                return Some("Stream key is required".to_string());
            }
        }
        crate::gstreamer::StreamDestination::CustomRtmp { url } => {
            if url.trim().is_empty() {
                return Some("RTMP URL is required".to_string());
            }
            if !url.starts_with("rtmp://") && !url.starts_with("rtmps://") {
                return Some("RTMP URL must start with rtmp:// or rtmps://".to_string());
            }
        }
    }
    None
}
```

- [ ] **Step 2: Update encoder_config_from_settings to build from stream or record settings**

Replace the existing `encoder_config_from_settings` with two functions:

```rust
fn stream_encoder_config(state: &AppState) -> EncoderConfig {
    let (width, height) = parse_resolution(&state.settings.video.output_resolution);
    let bitrate = if state.settings.stream.quality_preset == crate::gstreamer::QualityPreset::Custom {
        state.settings.stream.bitrate_kbps
    } else {
        state.settings.stream.quality_preset.bitrate_kbps()
    };
    EncoderConfig {
        width,
        height,
        fps: state.settings.stream.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.stream.encoder,
    }
}

fn record_encoder_config(state: &AppState) -> EncoderConfig {
    let (width, height) = parse_resolution(&state.settings.video.output_resolution);
    let bitrate = if state.settings.record.quality_preset == crate::gstreamer::QualityPreset::Custom {
        state.settings.record.bitrate_kbps
    } else {
        state.settings.record.quality_preset.bitrate_kbps()
    };
    EncoderConfig {
        width,
        height,
        fps: state.settings.record.fps,
        bitrate_kbps: bitrate,
        encoder_type: state.settings.record.encoder,
    }
}
```

- [ ] **Step 3: Update Go Live button**

In `draw_go_live_button`, replace the stream start logic:

```rust
} else {
    // Validate before starting
    if let Some(error_msg) = validate_stream_settings(state) {
        state.active_errors.push(crate::gstreamer::GstError::EncodeFailure {
            message: error_msg,
        });
    } else {
        let _ = tx.try_send(crate::gstreamer::GstCommand::StartStream {
            destination: state.settings.stream.destination.clone(),
            stream_key: state.settings.stream.stream_key.clone(),
            encoder_config: stream_encoder_config(state),
        });
        state.stream_status = StreamStatus::Live {
            uptime_secs: 0.0,
            bitrate_kbps: 0.0,
            dropped_frames: 0,
        };
    }
}
```

- [ ] **Step 4: Update Record button with filename template and timer**

Replace the record start logic in `draw_record_button`:

```rust
} else {
    state.recording_counter += 1;
    let scene_name = "Main"; // TODO: get active scene name from state
    let filename = crate::settings::RecordSettings::expand_template(
        &state.settings.record.filename_template,
        scene_name,
        state.recording_counter,
    );
    let ext = match state.settings.record.format {
        crate::gstreamer::RecordingFormat::Mkv => "mkv",
        crate::gstreamer::RecordingFormat::Mp4 => "mp4",
    };
    let folder = if state.settings.record.output_folder.exists() {
        state.settings.record.output_folder.clone()
    } else {
        dirs::video_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    };
    let path = folder.join(format!("{filename}.{ext}"));

    let _ = tx.try_send(crate::gstreamer::GstCommand::StartRecording {
        path: path.clone(),
        format: state.settings.record.format,
        encoder_config: record_encoder_config(state),
    });
    state.recording_status = RecordingStatus::Recording { path };
    state.recording_started_at = Some(std::time::Instant::now());
}
```

Update stop recording to clear the timer:
```rust
if is_recording {
    let _ = tx.try_send(crate::gstreamer::GstCommand::StopRecording);
    state.recording_status = RecordingStatus::Idle;
    state.recording_started_at = None;
}
```

- [ ] **Step 5: Add recording timer display**

In the REC indicator section (where it shows "REC" text), add elapsed time:

```rust
if let Some(started) = state.recording_started_at {
    let elapsed = started.elapsed().as_secs();
    let h = elapsed / 3600;
    let m = (elapsed % 3600) / 60;
    let s = elapsed % 60;
    let label = format!("REC {:02}:{:02}:{:02}", h, m, s);
    // Use `label` instead of just "REC"
}
```

- [ ] **Step 6: Remove old UpdateEncoder sends**

Remove the two `UpdateEncoder` sends that were added as the earlier bugfix — they're no longer needed since config is passed directly in the commands.

- [ ] **Step 7: Run tests and build**

Run: `cargo build 2>&1 && cargo test -q 2>&1`
Expected: compiles and all tests pass

- [ ] **Step 8: Commit**

```bash
git add src/ui/toolbar.rs
git commit -m "feat: toolbar validation, recording timer, settings-driven stream/record"
```

---

### Task 8: Stream settings panel rewrite

**Files:**
- Modify: `src/ui/settings/stream.rs` (full rewrite)

- [ ] **Step 1: Rewrite stream settings draw function**

The function signature changes to also accept `available_encoders`:

```rust
pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut StreamSettings,
    available_encoders: &[crate::gstreamer::AvailableEncoder],
) -> bool {
```

Layout:
1. Destination dropdown (Twitch/YouTube/Custom RTMP)
2. Stream Key (password field) — shown for Twitch/YouTube
3. RTMP URL text input — shown for Custom RTMP
4. Separator
5. Encoder dropdown — only shows available encoders, recommended marked
6. Quality toggle buttons (Low/Medium/High/Custom) + bitrate DragValue when Custom
7. FPS toggle buttons (24/30/60)

Use `egui::ComboBox` for dropdowns and `ui.selectable_value()` or custom toggle buttons for presets.

**Encoder dropdown pattern** (reuse in record.rs too):

```rust
/// Draw encoder selection dropdown. Returns true if changed.
fn draw_encoder_dropdown(
    ui: &mut egui::Ui,
    selected: &mut crate::gstreamer::EncoderType,
    available: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;
    let current_label = available
        .iter()
        .find(|e| e.encoder_type == *selected)
        .map(|e| {
            let name = e.encoder_type.display_name();
            if e.is_recommended {
                format!("{name} — Recommended")
            } else {
                name.to_string()
            }
        })
        .unwrap_or_else(|| selected.display_name().to_string());

    egui::ComboBox::from_label("")
        .selected_text(&current_label)
        .show_ui(ui, |ui| {
            for enc in available {
                let label = if enc.is_recommended {
                    format!("{} — Recommended", enc.encoder_type.display_name())
                } else {
                    enc.encoder_type.display_name().to_string()
                };
                if ui.selectable_value(selected, enc.encoder_type, &label).changed() {
                    changed = true;
                }
            }
        });
    changed
}
```

**Quality preset toggle pattern:**

```rust
/// Draw quality preset toggle row. Returns true if changed.
fn draw_quality_presets(
    ui: &mut egui::Ui,
    preset: &mut crate::gstreamer::QualityPreset,
    custom_bitrate: &mut u32,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &p in crate::gstreamer::QualityPreset::all() {
            let label = match p {
                crate::gstreamer::QualityPreset::Low => "Low",
                crate::gstreamer::QualityPreset::Medium => "Medium",
                crate::gstreamer::QualityPreset::High => "High",
                crate::gstreamer::QualityPreset::Custom => "Custom",
            };
            if ui.selectable_label(*preset == p, label).clicked() {
                *preset = p;
                changed = true;
            }
        }
    });
    // Show bitrate info or custom input
    if *preset == crate::gstreamer::QualityPreset::Custom {
        if ui.add(egui::DragValue::new(custom_bitrate).range(500..=50000).suffix(" kbps")).changed() {
            changed = true;
        }
    } else {
        ui.label(egui::RichText::new(format!("{} kbps", preset.bitrate_kbps())).weak().size(11.0));
    }
    changed
}
```

**FPS toggle pattern:**

```rust
fn draw_fps_toggles(ui: &mut egui::Ui, fps: &mut u32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for &f in &[24u32, 30, 60] {
            if ui.selectable_label(*fps == f, f.to_string()).clicked() {
                *fps = f;
                changed = true;
            }
        }
    });
    changed
}
```

Extract these as shared helpers (e.g. in a `src/ui/settings/widgets.rs` or directly in `stream.rs` as `pub(super)` functions) so `record.rs` can reuse them.

- [ ] **Step 2: Update settings/mod.rs to pass available_encoders**

In `render_content_direct()`, when calling `stream::draw()`, pass the encoders from AppState:

```rust
SettingsCategory::StreamOutput => {
    stream::draw(ui, &mut state.settings.stream, &state.available_encoders)
}
```

This requires the `render_content_direct` function to have access to `AppState` (check existing pattern — it should already receive it or `&mut AppSettings`).

- [ ] **Step 3: Build and test**

Run: `cargo build 2>&1`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add src/ui/settings/stream.rs src/ui/settings/mod.rs
git commit -m "feat: stream settings panel with encoder/quality/fps controls"
```

---

### Task 9: Recording settings panel (new)

**Files:**
- Create: `src/ui/settings/record.rs`
- Modify: `src/ui/settings/mod.rs` (add Recording category, wire draw)

- [ ] **Step 1: Add Recording category to SettingsCategory enum**

In `src/ui/settings/mod.rs`, add `Recording` variant to `SettingsCategory` and add it to `SIDEBAR_GROUPS` in the Output group (next to `StreamOutput`).

- [ ] **Step 2: Create record.rs**

Create `src/ui/settings/record.rs`:

```rust
use egui::Ui;
use crate::settings::RecordSettings;

pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut RecordSettings,
    available_encoders: &[crate::gstreamer::AvailableEncoder],
) -> bool {
    let mut changed = false;

    // Format toggle (MKV / MP4)
    ui.label("Format");
    ui.horizontal(|ui| {
        if ui.selectable_label(
            matches!(settings.format, crate::gstreamer::RecordingFormat::Mkv),
            "MKV",
        ).clicked() {
            settings.format = crate::gstreamer::RecordingFormat::Mkv;
            changed = true;
        }
        if ui.selectable_label(
            matches!(settings.format, crate::gstreamer::RecordingFormat::Mp4),
            "MP4",
        ).clicked() {
            settings.format = crate::gstreamer::RecordingFormat::Mp4;
            changed = true;
        }
    });
    ui.label(egui::RichText::new("MKV is crash-safe").weak().size(11.0));

    ui.add_space(8.0);

    // Output folder
    ui.label("Output Folder");
    let folder_str = settings.output_folder.display().to_string();
    ui.horizontal(|ui| {
        ui.label(&folder_str);
        if ui.button("Browse").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_directory(&settings.output_folder)
                .pick_folder()
            {
                settings.output_folder = path;
                changed = true;
            }
        }
    });

    ui.add_space(8.0);

    // Filename template
    ui.label("Filename Template");
    if ui.text_edit_singleline(&mut settings.filename_template).changed() {
        changed = true;
    }
    // Live preview
    let preview = RecordSettings::expand_template(
        &settings.filename_template,
        "Main",
        1,
    );
    let ext = match settings.format {
        crate::gstreamer::RecordingFormat::Mkv => "mkv",
        crate::gstreamer::RecordingFormat::Mp4 => "mp4",
    };
    ui.label(egui::RichText::new(format!("Preview: {preview}.{ext}")).weak().size(11.0));

    ui.separator();

    // Encoder, quality, FPS — reuse shared helpers from stream.rs
    ui.label("Encoder");
    if draw_encoder_dropdown(ui, &mut settings.encoder, available_encoders) {
        changed = true;
    }

    ui.add_space(8.0);
    ui.label("Quality");
    if draw_quality_presets(ui, &mut settings.quality_preset, &mut settings.bitrate_kbps) {
        changed = true;
    }

    ui.add_space(8.0);
    ui.label("FPS");
    if draw_fps_toggles(ui, &mut settings.fps) {
        changed = true;
    }

    changed
}
```

Note: `rfd` (Rust File Dialog) may need to be added as a dependency. Check if it's already in `Cargo.toml`. If not, add `rfd = "0.15"`. If a native dialog dependency is undesirable, use a text input for the folder path instead.

- [ ] **Step 3: Wire recording settings in mod.rs**

Add `mod record;` declaration and dispatch in `render_content_direct`:

```rust
SettingsCategory::Recording => {
    record::draw(ui, &mut state.settings.record, &state.available_encoders)
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build 2>&1`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add src/ui/settings/record.rs src/ui/settings/mod.rs
git commit -m "feat: recording settings panel with format, output folder, filename template"
```

---

### Task 10: Update re-exports, clean up dead code, final integration test

**Files:**
- Modify: `src/gstreamer/mod.rs` (clean up re-exports — remove StreamConfig)
- Various files (remove any remaining dead code warnings)

- [ ] **Step 1: Clean up re-exports**

Remove `StreamConfig` from `src/gstreamer/mod.rs` re-exports since the struct was removed.

- [ ] **Step 2: Remove dead_code allows that are no longer needed**

Check for `#[allow(dead_code)]` on `RecordingFormat::Mp4` — it's now used in settings UI, so remove the allow.

- [ ] **Step 3: Run full test suite**

Run: `cargo test -q 2>&1`
Expected: all tests pass

- [ ] **Step 4: Run clippy**

Run: `cargo clippy 2>&1`
Expected: no warnings related to changes

- [ ] **Step 5: Manual smoke test**

Run: `RUST_LOG=info cargo run`

Test:
1. Open Settings → Stream: verify encoder dropdown shows detected encoders with "Recommended" tag
2. Open Settings → Recording: verify format toggle, output folder, filename template preview
3. Click Record: verify file is created with template-based name, REC timer shows in toolbar
4. Stop Record: verify timer stops, file is valid MKV
5. Set a stream key, click Go Live (to a test endpoint or expect connection failure with proper error message)

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "chore: clean up dead code and re-exports after recording/streaming rework"
```
