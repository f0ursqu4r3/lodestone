# Audio Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mic + system audio capture, AAC encoding, mixed streaming, multi-track recording, and audio level metering to Lodestone.

**Architecture:** Two independent GStreamer audio capture pipelines (mic + system/BlackHole) produce PCM samples. For streaming, `audiomixer` mixes them into one stereo track before AAC encoding into `flvmux`. For recording, each source gets its own AAC encode chain and track in the container. The `level` element provides peak/RMS metering sent to the UI via a `watch` channel.

**Tech Stack:** Rust, gstreamer-rs 0.23, gstreamer-audio 0.23, GStreamer `osxaudiosrc`, `audiomixer`, `avenc_aac`, `aacparse`, `level`

**Spec:** `docs/superpowers/specs/2026-03-22-audio-capture-design.md`

---

## File Structure

### New files
- `src/gstreamer/devices.rs` — audio input device enumeration via GStreamer DeviceMonitor

### Modified files
- `Cargo.toml` — add `gstreamer-audio = "0.23"`
- `src/gstreamer/mod.rs` — add `pub mod devices;`, update re-exports
- `src/gstreamer/types.rs` — add `AudioLevelUpdate`, `AudioLevels`, `AudioDevice`
- `src/gstreamer/commands.rs` — add `AudioSourceKind`, `AudioEncoderConfig`, audio `GstCommand` variants, audio level channel
- `src/gstreamer/error.rs` — add `AudioCaptureFailure` variant
- `src/gstreamer/capture.rs` — add `build_audio_capture_pipeline()`
- `src/gstreamer/encode.rs` — new return types (`StreamPipelineHandles`, `RecordPipelineHandles`), audio appsrc + audiomixer + aacparse elements
- `src/gstreamer/thread.rs` — audio pipeline management, audio forwarding to encode, level reporting, multi-appsrc EOS
- `src/state.rs` — replace `AudioLevel`/`Vec<AudioLevel>` with `AudioLevelUpdate`, add `available_audio_devices`
- `src/ui/audio_mixer.rs` — full mixer UI with VU meters, faders, mute toggles
- `src/ui/settings_window.rs` — runtime device enumeration replaces hardcoded list
- `src/main.rs` — wire audio level watch channel, poll audio levels

---

### Task 1: Add gstreamer-audio dependency and audio types

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/gstreamer/types.rs`
- Modify: `src/gstreamer/commands.rs`
- Modify: `src/gstreamer/error.rs`
- Modify: `src/gstreamer/mod.rs`

- [ ] **Step 1: Add `gstreamer-audio` to Cargo.toml**

Add to `[dependencies]`:
```toml
gstreamer-audio = "0.23"
```

Note: `glib` is a transitive dependency of `gstreamer` and re-exported as `gstreamer::glib`. Use `gstreamer::gstreamer::glib::ValueArray` in level parsing — no extra Cargo dep needed.

- [ ] **Step 2: Add audio types to `src/gstreamer/types.rs`**

```rust
/// Audio level data from the GStreamer `level` element.
#[derive(Debug, Clone, Default)]
pub struct AudioLevelUpdate {
    pub mic: Option<AudioLevels>,
    pub system: Option<AudioLevels>,
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
```

- [ ] **Step 3: Add audio command types to `src/gstreamer/commands.rs`**

Add `AudioSourceKind` enum:
```rust
#[derive(Debug, Clone, Copy)]
pub enum AudioSourceKind {
    Mic,
    System,
}
```

Add `AudioEncoderConfig` struct:
```rust
#[derive(Debug, Clone)]
pub struct AudioEncoderConfig {
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        Self { bitrate_kbps: 128, sample_rate: 48000, channels: 2 }
    }
}
```

Add new variants to `GstCommand`:
```rust
SetAudioDevice { source: AudioSourceKind, device_uid: String },
SetAudioVolume { source: AudioSourceKind, volume: f32 },
SetAudioMuted { source: AudioSourceKind, muted: bool },
```

Add `audio_level_tx`/`audio_level_rx` and `devices_tx`/`devices_rx` to the channel structs and `create_channels()`:
```rust
// In GstChannels:
pub audio_level_rx: watch::Receiver<AudioLevelUpdate>,
pub devices_rx: watch::Receiver<Vec<AudioDevice>>,

// In GstThreadChannels:
pub audio_level_tx: watch::Sender<AudioLevelUpdate>,
pub devices_tx: watch::Sender<Vec<AudioDevice>>,
```

Create the watch channels in `create_channels()`:
```rust
let (audio_level_tx, audio_level_rx) = watch::channel(AudioLevelUpdate::default());
let (devices_tx, devices_rx) = watch::channel(Vec::new());
```

Import `AudioDevice` and `AudioLevelUpdate` from `super::types` at the top of `commands.rs`.

- [ ] **Step 4: Add `AudioCaptureFailure` to `src/gstreamer/error.rs`**

Add variant to `GstError`:
```rust
/// Audio capture device failed (mic unplugged, permission denied).
AudioCaptureFailure { message: String },
```

Add to `Display` impl:
```rust
Self::AudioCaptureFailure { message } => write!(f, "Audio capture failed: {message}"),
```

- [ ] **Step 5: Update `src/gstreamer/mod.rs` re-exports**

Add to the `pub use` lines:
```rust
pub use commands::{AudioSourceKind, AudioEncoderConfig};
pub use types::{AudioLevelUpdate, AudioLevels, AudioDevice};
```

- [ ] **Step 6: Run `cargo check` and `cargo test`**

Expected: compiles, all tests pass

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/gstreamer/types.rs src/gstreamer/commands.rs src/gstreamer/error.rs src/gstreamer/mod.rs
git commit -m "feat: add audio types, commands, and gstreamer-audio dependency"
```

---

### Task 2: Implement device enumeration

**Files:**
- Create: `src/gstreamer/devices.rs`
- Modify: `src/gstreamer/mod.rs`

- [ ] **Step 1: Create `src/gstreamer/devices.rs`**

```rust
use anyhow::{Context, Result};
use gstreamer::prelude::*;

use super::types::AudioDevice;

/// Known virtual audio device names used for system audio loopback.
const LOOPBACK_DEVICE_NAMES: &[&str] = &["BlackHole", "Soundflower", "Loopback"];

/// Enumerate available audio input devices using GStreamer's DeviceMonitor.
pub fn enumerate_audio_input_devices() -> Result<Vec<AudioDevice>> {
    let monitor = gstreamer::DeviceMonitor::new();

    // Filter for audio source devices
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
            .and_then(|props| {
                props.value("device.uid")
                    .ok()
                    .and_then(|v| v.get::<String>().ok())
            })
            .unwrap_or_else(|| name.clone());

        let is_loopback = LOOPBACK_DEVICE_NAMES
            .iter()
            .any(|known| name.contains(known));

        result.push(AudioDevice { uid, name, is_loopback });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerate_does_not_panic() {
        gstreamer::init().unwrap();
        // May return empty list on CI without audio devices
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
    fn loopback_detection() {
        let device = AudioDevice {
            uid: "BlackHole2ch_UID".to_string(),
            name: "BlackHole 2ch".to_string(),
            is_loopback: LOOPBACK_DEVICE_NAMES.iter().any(|known| "BlackHole 2ch".contains(known)),
        };
        assert!(device.is_loopback);

        let mic = AudioDevice {
            uid: "BuiltInMicrophoneDevice".to_string(),
            name: "Built-in Microphone".to_string(),
            is_loopback: LOOPBACK_DEVICE_NAMES.iter().any(|known| "Built-in Microphone".contains(known)),
        };
        assert!(!mic.is_loopback);
    }
}
```

- [ ] **Step 2: Add `pub mod devices;` to `src/gstreamer/mod.rs`**

- [ ] **Step 3: Run tests**

Run: `cargo test gstreamer::devices`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/devices.rs src/gstreamer/mod.rs
git commit -m "feat: add audio device enumeration via GStreamer DeviceMonitor"
```

---

### Task 3: Build the audio capture pipeline

**Files:**
- Modify: `src/gstreamer/capture.rs`

- [ ] **Step 1: Add `build_audio_capture_pipeline()` to `src/gstreamer/capture.rs`**

Add the function below the existing `build_capture_pipeline()`:

```rust
use gstreamer_app::AppSink;
use super::commands::AudioSourceKind;

/// Build an audio capture pipeline for the given device.
///
/// Pipeline: osxaudiosrc → audioconvert → audioresample → volume → level → appsink
/// Returns (pipeline, appsink, volume_element_name).
pub fn build_audio_capture_pipeline(
    source_kind: AudioSourceKind,
    device_uid: &str,
    sample_rate: u32,
) -> Result<(gstreamer::Pipeline, AppSink, String)> {
    let name = match source_kind {
        AudioSourceKind::Mic => "mic-capture",
        AudioSourceKind::System => "system-capture",
    };
    let pipeline = gstreamer::Pipeline::with_name(name);

    let src = gstreamer::ElementFactory::make("osxaudiosrc")
        .name(&format!("{name}-src"))
        .property("device", device_uid)
        .build()
        .context(format!("Failed to create osxaudiosrc for {name}"))?;

    let convert = gstreamer::ElementFactory::make("audioconvert")
        .name(&format!("{name}-convert"))
        .build()
        .context("Failed to create audioconvert")?;

    let resample = gstreamer::ElementFactory::make("audioresample")
        .name(&format!("{name}-resample"))
        .build()
        .context("Failed to create audioresample")?;

    let volume_name = format!("{name}-volume");
    let volume = gstreamer::ElementFactory::make("volume")
        .name(&volume_name)
        .build()
        .context("Failed to create volume")?;

    let level = gstreamer::ElementFactory::make("level")
        .name(&format!("{name}-level"))
        .property("interval", 50_000_000u64) // 50ms in nanoseconds
        .property("post-messages", true)
        .build()
        .context("Failed to create level")?;

    let caps = gstreamer_audio::AudioCapsBuilder::new()
        .format(gstreamer_audio::AudioFormat::S16le)
        .rate(sample_rate as i32)
        .channels(2)
        .build();

    let appsink = AppSink::builder()
        .name(&format!("{name}-sink"))
        .caps(&caps)
        .max_buffers(4)
        .drop(true)
        .build();

    pipeline
        .add_many([&src, &convert, &resample, &volume, &level, appsink.upcast_ref()])
        .context("Failed to add audio capture elements")?;

    gstreamer::Element::link_many([&src, &convert, &resample, &volume, &level, appsink.upcast_ref()])
        .context("Failed to link audio capture elements")?;

    Ok((pipeline, appsink, volume_name))
}
```

- [ ] **Step 2: Add test**

```rust
#[test]
fn build_audio_capture_pipeline_creates_valid_pipeline() {
    gstreamer::init().unwrap();
    let result = build_audio_capture_pipeline(
        crate::gstreamer::commands::AudioSourceKind::Mic,
        "default",
        48000,
    );
    match result {
        Ok((pipeline, appsink, vol_name)) => {
            assert!(pipeline.name().starts_with("mic-capture"));
            assert!(vol_name.contains("volume"));
            drop(appsink);
            let _ = pipeline.set_state(gstreamer::State::Null);
        }
        Err(e) => {
            eprintln!("Skipping audio capture pipeline test (missing plugins): {e}");
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test gstreamer::capture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/capture.rs
git commit -m "feat: add audio capture pipeline builder"
```

---

### Task 4: Add audio to encode pipelines

**Files:**
- Modify: `src/gstreamer/encode.rs`

- [ ] **Step 1: Add handle structs and audio helper to `src/gstreamer/encode.rs`**

Add at the top (after imports):

```rust
use gstreamer_app::AppSrc;
use super::commands::AudioEncoderConfig;

/// Handles for the streaming pipeline (mixed audio via audiomixer).
pub struct StreamPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub audio_appsrc_mic: AppSrc,
    pub audio_appsrc_system: Option<AppSrc>,
}

/// Handles for the recording pipeline (separate audio tracks).
pub struct RecordPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub mic_appsrc: AppSrc,
    pub system_appsrc: Option<AppSrc>,
}
```

Add audio caps helper:
```rust
fn make_audio_appsrc_caps(config: &AudioEncoderConfig) -> gstreamer::Caps {
    gstreamer_audio::AudioCapsBuilder::new()
        .format(gstreamer_audio::AudioFormat::S16le)
        .rate(config.sample_rate as i32)
        .channels(config.channels as i32)
        .build()
}
```

- [ ] **Step 2: Create `build_stream_pipeline_with_audio()`**

New function that builds the full pipeline with audiomixer:

```rust
/// Build a streaming pipeline with video + mixed audio.
///
/// Video: appsrc → videoconvert → vtenc_h264 → h264parse → flvmux → rtmpsink
/// Audio: mic_appsrc + system_appsrc → audiomixer → avenc_aac → aacparse → flvmux
pub fn build_stream_pipeline_with_audio(
    video_config: &EncoderConfig,
    audio_config: &AudioEncoderConfig,
    rtmp_url: &str,
    has_system_audio: bool,
) -> Result<StreamPipelineHandles> {
    let (pipeline, video_appsrc, video_last) =
        build_encode_chain(video_config, "encode-stream-pipeline")?;

    // Audio: mic appsrc
    let audio_caps = make_audio_appsrc_caps(audio_config);
    let mic_appsrc = AppSrc::builder()
        .name("stream-mic-src")
        .caps(&audio_caps)
        .format(gstreamer::Format::Time)
        .is_live(true)
        .build();

    // Audio: audiomixer → audioconvert → avenc_aac → aacparse
    let mixer = gstreamer::ElementFactory::make("audiomixer")
        .name("stream-mixer")
        .build()
        .context("Failed to create audiomixer")?;

    let audio_convert = gstreamer::ElementFactory::make("audioconvert")
        .name("stream-audio-convert")
        .build()
        .context("Failed to create audioconvert")?;

    let aac_enc = gstreamer::ElementFactory::make("avenc_aac")
        .name("stream-aac-enc")
        .build()
        .context("Failed to create avenc_aac")?;
    // Set bitrate after creation to handle property type differences
    aac_enc.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i64);

    let aac_parse = gstreamer::ElementFactory::make("aacparse")
        .name("stream-aac-parse")
        .build()
        .context("Failed to create aacparse")?;

    // Mux + sink
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

    // Add all elements
    pipeline.add_many([
        mic_appsrc.upcast_ref(), &mixer, &audio_convert, &aac_enc, &aac_parse, &mux, &sink,
    ]).context("Failed to add stream audio elements")?;

    // Link video to mux using explicit pad request
    let video_last_elem = pipeline.by_name(&video_last).expect("video parser exists");
    let video_mux_pad = mux.request_pad_simple("video")
        .context("Failed to request video pad on flvmux")?;
    video_last_elem.static_pad("src").unwrap().link(&video_mux_pad)
        .context("Failed to link video to flvmux")?;
    mux.link(&sink).context("Failed to link flvmux to rtmpsink")?;

    // Link mic appsrc → audiomixer
    mic_appsrc.upcast_ref::<gstreamer::Element>().link(&mixer)
        .context("Failed to link mic to audiomixer")?;

    // Link audiomixer → audioconvert → aac → aacparse → mux
    gstreamer::Element::link_many([&mixer, &audio_convert, &aac_enc, &aac_parse])
        .context("Failed to link audio encode chain")?;
    // Request audio pad on mux
    let audio_mux_pad = mux.request_pad_simple("audio")
        .context("Failed to request audio pad on flvmux")?;
    let aac_parse_src = aac_parse.static_pad("src")
        .context("aacparse has no src pad")?;
    aac_parse_src.link(&audio_mux_pad)
        .context("Failed to link aacparse to flvmux audio pad")?;

    // System audio appsrc (optional)
    let system_appsrc = if has_system_audio {
        let sys_appsrc = AppSrc::builder()
            .name("stream-system-src")
            .caps(&audio_caps)
            .format(gstreamer::Format::Time)
            .is_live(true)
            .build();
        pipeline.add(sys_appsrc.upcast_ref()).context("Failed to add system appsrc")?;
        sys_appsrc.upcast_ref::<gstreamer::Element>().link(&mixer)
            .context("Failed to link system to audiomixer")?;
        Some(sys_appsrc)
    } else {
        None
    };

    Ok(StreamPipelineHandles {
        pipeline,
        video_appsrc,
        audio_appsrc_mic: mic_appsrc,
        audio_appsrc_system: system_appsrc,
    })
}
```

- [ ] **Step 3: Create `build_record_pipeline_with_audio()`**

```rust
/// Build a recording pipeline with video + separate audio tracks.
///
/// Video: appsrc → videoconvert → vtenc_h264 → h264parse → mux → filesink
/// Mic:   appsrc → audioconvert → avenc_aac → aacparse → mux (track 1)
/// System: appsrc → audioconvert → avenc_aac → aacparse → mux (track 2)
pub fn build_record_pipeline_with_audio(
    video_config: &EncoderConfig,
    audio_config: &AudioEncoderConfig,
    path: &Path,
    format: RecordingFormat,
    has_system_audio: bool,
) -> Result<RecordPipelineHandles> {
    let (pipeline, video_appsrc, video_last) =
        build_encode_chain(video_config, "encode-record-pipeline")?;

    let mux = match format {
        RecordingFormat::Mkv => gstreamer::ElementFactory::make("matroskamux")
            .name("record-mux").build().context("Failed to create matroskamux")?,
        RecordingFormat::Mp4 => gstreamer::ElementFactory::make("mp4mux")
            .name("record-mux")
            .property_from_str("fragment-duration", "1000")
            .build().context("Failed to create mp4mux")?,
    };

    let filesink = gstreamer::ElementFactory::make("filesink")
        .name("record-sink")
        .property("location", path.to_str().unwrap_or("recording.mkv"))
        .build()
        .context("Failed to create filesink")?;

    pipeline.add_many([&mux, &filesink]).context("Failed to add mux/filesink")?;

    // Link video to mux
    let video_last_elem = pipeline.by_name(&video_last).expect("video parser exists");
    gstreamer::Element::link_many([&video_last_elem, &mux, &filesink])
        .context("Failed to link video to mux")?;

    let audio_caps = make_audio_appsrc_caps(audio_config);

    // Mic audio track
    let mic_appsrc = AppSrc::builder()
        .name("record-mic-src").caps(&audio_caps)
        .format(gstreamer::Format::Time).is_live(true).build();

    let mic_convert = gstreamer::ElementFactory::make("audioconvert")
        .name("record-mic-convert").build().context("audioconvert")?;
    let mic_enc = gstreamer::ElementFactory::make("avenc_aac")
        .name("record-mic-enc")
        .build().context("avenc_aac")?;
    mic_enc.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i64);
    let mic_parse = gstreamer::ElementFactory::make("aacparse")
        .name("record-mic-parse").build().context("aacparse")?;

    pipeline.add_many([mic_appsrc.upcast_ref(), &mic_convert, &mic_enc, &mic_parse])
        .context("Failed to add mic audio elements")?;
    gstreamer::Element::link_many([mic_appsrc.upcast_ref(), &mic_convert, &mic_enc, &mic_parse])
        .context("Failed to link mic audio chain")?;

    let mic_mux_pad = mux.request_pad_simple("audio_%u")
        .context("Failed to request mic audio pad")?;
    mic_parse.static_pad("src").unwrap().link(&mic_mux_pad)
        .context("Failed to link mic aacparse to mux")?;

    // System audio track (optional)
    let system_appsrc = if has_system_audio {
        let sys_appsrc = AppSrc::builder()
            .name("record-system-src").caps(&audio_caps)
            .format(gstreamer::Format::Time).is_live(true).build();

        let sys_convert = gstreamer::ElementFactory::make("audioconvert")
            .name("record-system-convert").build().context("audioconvert")?;
        let sys_enc = gstreamer::ElementFactory::make("avenc_aac")
            .name("record-system-enc")
            .build().context("avenc_aac")?;
        sys_enc.set_property("bitrate", (audio_config.bitrate_kbps * 1000) as i64);
        let sys_parse = gstreamer::ElementFactory::make("aacparse")
            .name("record-system-parse").build().context("aacparse")?;

        pipeline.add_many([sys_appsrc.upcast_ref(), &sys_convert, &sys_enc, &sys_parse])
            .context("Failed to add system audio elements")?;
        gstreamer::Element::link_many([sys_appsrc.upcast_ref(), &sys_convert, &sys_enc, &sys_parse])
            .context("Failed to link system audio chain")?;

        let sys_mux_pad = mux.request_pad_simple("audio_%u")
            .context("Failed to request system audio pad")?;
        sys_parse.static_pad("src").unwrap().link(&sys_mux_pad)
            .context("Failed to link system aacparse to mux")?;

        Some(sys_appsrc)
    } else {
        None
    };

    Ok(RecordPipelineHandles {
        pipeline,
        video_appsrc,
        mic_appsrc,
        system_appsrc,
    })
}
```

- [ ] **Step 4: Update existing tests and add new ones**

Keep the old `build_stream_pipeline` and `build_record_pipeline` functions for backwards compat (they're still used by existing code until Task 6 migrates). Add tests for the new functions:

```rust
#[test]
fn build_stream_with_audio_creates_valid_pipeline() {
    gstreamer::init().unwrap();
    let vc = EncoderConfig::default();
    let ac = AudioEncoderConfig::default();
    let result = build_stream_pipeline_with_audio(&vc, &ac, "rtmp://localhost/test", false);
    match result {
        Ok(handles) => {
            assert!(handles.pipeline.name().starts_with("encode"));
            assert!(handles.audio_appsrc_system.is_none());
            let _ = handles.pipeline.set_state(gstreamer::State::Null);
        }
        Err(e) => eprintln!("Skipping: {e}"),
    }
}

#[test]
fn build_record_with_audio_creates_valid_pipeline() {
    gstreamer::init().unwrap();
    let vc = EncoderConfig::default();
    let ac = AudioEncoderConfig::default();
    let path = std::path::PathBuf::from("/tmp/test_audio_record.mkv");
    let result = build_record_pipeline_with_audio(&vc, &ac, &path, RecordingFormat::Mkv, false);
    match result {
        Ok(handles) => {
            assert!(handles.pipeline.name().starts_with("encode"));
            assert!(handles.system_appsrc.is_none());
            let _ = handles.pipeline.set_state(gstreamer::State::Null);
        }
        Err(e) => eprintln!("Skipping: {e}"),
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test gstreamer::encode`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/encode.rs
git commit -m "feat: add audio-aware stream and record pipeline builders"
```

---

### Task 5: Update state.rs — replace AudioLevel with AudioLevelUpdate

**Files:**
- Modify: `src/state.rs`

- [ ] **Step 1: Replace `AudioLevel` and `audio_levels` field**

Remove the `AudioLevel` struct, its `impl`, and `#[allow(dead_code)]` annotations. Replace:

```rust
// Old:
pub audio_levels: Vec<AudioLevel>,

// New:
pub audio_levels: crate::gstreamer::AudioLevelUpdate,
pub available_audio_devices: Vec<crate::gstreamer::AudioDevice>,
```

Update `Default` impl:
```rust
audio_levels: crate::gstreamer::AudioLevelUpdate::default(),
available_audio_devices: Vec::new(),
```

Remove the `audio_level_clamping` test and update `default_app_state` test if needed.

- [ ] **Step 2: Run tests**

Run: `cargo test`
Expected: PASS (the audio_level_clamping test is removed since `AudioLevel` is gone)

- [ ] **Step 3: Commit**

```bash
git add src/state.rs
git commit -m "refactor: replace AudioLevel with AudioLevelUpdate in AppState"
```

---

### Task 6: Wire audio into the GStreamer thread

**Files:**
- Modify: `src/gstreamer/thread.rs`
- Modify: `src/main.rs`

This is the largest task — it wires audio capture pipelines, audio forwarding to encode, level metering, and the new pipeline handle types into the thread loop.

- [ ] **Step 1: Update `GstThread` struct fields**

Replace `stream_appsrc`/`record_appsrc` with handle structs. Add audio pipeline fields:

```rust
struct GstThread {
    channels: GstThreadChannels,
    capture_pipeline: Option<gstreamer::Pipeline>,
    capture_appsink: Option<AppSink>,
    // Audio capture
    mic_pipeline: Option<gstreamer::Pipeline>,
    mic_appsink: Option<AppSink>,
    mic_volume_name: Option<String>,
    system_pipeline: Option<gstreamer::Pipeline>,
    system_appsink: Option<AppSink>,
    system_volume_name: Option<String>,
    has_system_audio: bool,
    // Encode pipelines (now with audio handles)
    stream_handles: Option<StreamPipelineHandles>,
    record_handles: Option<RecordPipelineHandles>,
    // Config
    encoder_config: EncoderConfig,
    audio_encoder_config: AudioEncoderConfig,
}
```

- [ ] **Step 2: Add `stop_audio_capture()`, `start_audio_capture()` methods**

```rust
fn stop_audio_capture(&mut self, kind: AudioSourceKind) {
    match kind {
        AudioSourceKind::Mic => {
            if let Some(pipeline) = self.mic_pipeline.take() {
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            self.mic_appsink = None;
            self.mic_volume_name = None;
        }
        AudioSourceKind::System => {
            if let Some(pipeline) = self.system_pipeline.take() {
                let _ = pipeline.set_state(gstreamer::State::Null);
            }
            self.system_appsink = None;
            self.system_volume_name = None;
            self.has_system_audio = false;
        }
    }
}

fn start_audio_capture(&mut self, kind: AudioSourceKind, device_uid: &str) {
    self.stop_audio_capture(kind);
    match build_audio_capture_pipeline(kind, device_uid, self.audio_encoder_config.sample_rate) {
        Ok((pipeline, appsink, volume_name)) => {
            if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
                let _ = self.channels.error_tx.send(GstError::AudioCaptureFailure {
                    message: format!("Failed to start {kind:?} audio: {e}"),
                });
                return;
            }
            match kind {
                AudioSourceKind::Mic => {
                    self.mic_pipeline = Some(pipeline);
                    self.mic_appsink = Some(appsink);
                    self.mic_volume_name = Some(volume_name);
                }
                AudioSourceKind::System => {
                    self.system_pipeline = Some(pipeline);
                    self.system_appsink = Some(appsink);
                    self.system_volume_name = Some(volume_name);
                    self.has_system_audio = true;
                }
            }
            log::info!("{kind:?} audio capture started");
        }
        Err(e) => {
            let _ = self.channels.error_tx.send(GstError::AudioCaptureFailure {
                message: format!("{e}"),
            });
        }
    }
}
```

- [ ] **Step 3: Update `handle_command` for audio commands**

Add match arms for `SetAudioDevice`, `SetAudioVolume`, `SetAudioMuted`. Volume/mute update the `volume` element property on the appropriate pipeline by looking it up via `pipeline.by_name(&volume_name)`.

- [ ] **Step 4: Update `handle_command` for StartStream/StartRecording**

Replace calls to `build_stream_pipeline` / `build_record_pipeline` with the new `build_stream_pipeline_with_audio` / `build_record_pipeline_with_audio`. Store the returned handles in `stream_handles` / `record_handles`.

- [ ] **Step 5: Update `stop_pipeline` for multi-appsrc EOS**

When stopping a stream or record, send EOS on all appsrcs (video + mic + system) from the handles before waiting for the pipeline EOS.

- [ ] **Step 6: Add audio sample forwarding in the run loop**

After the video frame pull, add:
```rust
// Pull mic audio and forward to encode pipelines
if let Some(appsink) = &self.mic_appsink {
    if let Some(sample) = appsink.try_pull_sample(gstreamer::ClockTime::from_mseconds(0)) {
        if let Some(buffer) = sample.buffer() {
            if let Ok(map) = buffer.map_readable() {
                let pts = gstreamer::ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);
                if let Some(ref handles) = self.stream_handles {
                    Self::push_to_encode(&handles.audio_appsrc_mic, map.as_slice(), pts);
                }
                if let Some(ref handles) = self.record_handles {
                    Self::push_to_encode(&handles.mic_appsrc, map.as_slice(), pts);
                }
            }
        }
    }
}
// Same pattern for system audio appsink → stream/record system appsrcs
```

- [ ] **Step 7: Add level metering via bus messages**

After the audio pulls, check the bus of each audio pipeline for `level` messages and update `audio_level_tx`:

```rust
fn poll_audio_levels(&self) {
    let mut update = AudioLevelUpdate::default();
    if let Some(ref pipeline) = self.mic_pipeline {
        if let Some(levels) = Self::read_level_from_bus(pipeline) {
            update.mic = Some(levels);
        }
    }
    if let Some(ref pipeline) = self.system_pipeline {
        if let Some(levels) = Self::read_level_from_bus(pipeline) {
            update.system = Some(levels);
        }
    }
    if update.mic.is_some() || update.system.is_some() {
        let _ = self.channels.audio_level_tx.send(update);
    }
}

fn read_level_from_bus(pipeline: &gstreamer::Pipeline) -> Option<AudioLevels> {
    let bus = pipeline.bus()?;
    let mut result = None;
    while let Some(msg) = bus.pop() {
        if let gstreamer::MessageView::Element(elem) = msg.view() {
            if let Some(structure) = elem.structure() {
                if structure.name().as_str() == "level" {
                    // The level element emits peak/rms as arrays of f64 (one per channel).
                    // We take the first channel's value.
                    let peak = structure.value("peak").ok()
                        .and_then(|v| v.get::<gstreamer::glib::ValueArray>().ok())
                        .and_then(|arr| arr.first().and_then(|v| v.get::<f64>().ok()))
                        .unwrap_or(-60.0) as f32;
                    let rms = structure.value("rms").ok()
                        .and_then(|v| v.get::<gstreamer::glib::ValueArray>().ok())
                        .and_then(|arr| arr.first().and_then(|v| v.get::<f64>().ok()))
                        .unwrap_or(-60.0) as f32;
                    result = Some(AudioLevels { peak_db: peak, rms_db: rms });
                }
            }
        }
    }
    result // Return the most recent level message
}
```

- [ ] **Step 8: Update `run()` to start mic capture on launch and enumerate devices**

At the start of `run()`, after starting video capture:
```rust
// Enumerate audio devices and start mic capture with default device
match devices::enumerate_audio_input_devices() {
    Ok(devices) => {
        // Find default mic (first non-loopback device)
        if let Some(mic) = devices.iter().find(|d| !d.is_loopback) {
            self.start_audio_capture(AudioSourceKind::Mic, &mic.uid);
        }
        // Find loopback device for system audio
        if let Some(loopback) = devices.iter().find(|d| d.is_loopback) {
            self.start_audio_capture(AudioSourceKind::System, &loopback.uid);
        }
        // Send device list to main thread via devices_tx watch channel
        let _ = self.channels.devices_tx.send(devices.clone());
        log::info!("Found {} audio devices", devices.len());
    }
    Err(e) => log::warn!("Failed to enumerate audio devices: {e}"),
}
```

- [ ] **Step 9: Update `src/main.rs` to poll audio levels and devices**

In `about_to_wait()`, add polling of `audio_level_rx` and `devices_rx` alongside the existing frame/error polling:
```rust
// Poll audio levels
if let Some(ref channels) = self.gst_channels {
    if channels.audio_level_rx.has_changed().unwrap_or(false) {
        let levels = channels.audio_level_rx.borrow().clone();
        let mut state = self.state.lock().unwrap();
        state.audio_levels = levels;
    }
    // Poll device list
    if channels.devices_rx.has_changed().unwrap_or(false) {
        let devices = channels.devices_rx.borrow().clone();
        let mut state = self.state.lock().unwrap();
        state.available_audio_devices = devices;
    }
}
```

- [ ] **Step 10: Update existing thread.rs tests**

The `gst_thread_new_has_defaults` test references `stream_appsrc` / `record_appsrc` which no longer exist. Update it to check `stream_handles` / `record_handles` instead:
```rust
assert!(thread.stream_handles.is_none());
assert!(thread.record_handles.is_none());
assert!(thread.mic_pipeline.is_none());
assert!(thread.system_pipeline.is_none());
```

- [ ] **Step 11: Run tests and verify compilation**

Run: `cargo check` then `cargo test`

- [ ] **Step 12: Commit**

```bash
git add src/gstreamer/thread.rs src/main.rs
git commit -m "feat: wire audio capture, encoding, and level metering into GStreamer thread"
```

---

### Task 7: Implement the audio mixer UI

**Files:**
- Modify: `src/ui/audio_mixer.rs`

- [ ] **Step 1: Replace placeholder with real mixer**

Replace the entire `draw` function with a real mixer showing Mic and System channels:

```rust
use crate::gstreamer::{AudioSourceKind, GstCommand};
use crate::state::AppState;
use crate::ui::layout::PanelId;

pub fn draw(ui: &mut egui::Ui, state: &mut AppState, _panel_id: PanelId) {
    ui.horizontal(|ui| {
        // Mic channel
        draw_channel_strip(ui, state, "Mic", AudioSourceKind::Mic,
            state.audio_levels.mic.as_ref());

        ui.separator();

        // System channel
        if state.available_audio_devices.iter().any(|d| d.is_loopback) {
            draw_channel_strip(ui, state, "System", AudioSourceKind::System,
                state.audio_levels.system.as_ref());
        } else {
            ui.vertical(|ui| {
                ui.set_min_width(60.0);
                ui.label("System");
                ui.add_space(10.0);
                ui.label("Install\nBlackHole\nfor system\naudio");
            });
        }
    });
}

fn draw_channel_strip(
    ui: &mut egui::Ui,
    state: &AppState,
    name: &str,
    kind: AudioSourceKind,
    levels: Option<&crate::gstreamer::AudioLevels>,
) {
    let current_db = levels.map(|l| l.rms_db).unwrap_or(-60.0);
    let peak_db = levels.map(|l| l.peak_db).unwrap_or(-60.0);

    ui.vertical(|ui| {
        ui.set_min_width(60.0);
        ui.label(name);

        // Volume fader (stored in egui memory since we can't mutate AppState for this)
        let vol_id = egui::Id::new(("audio_vol", name));
        let mut volume: f32 = ui.memory(|m| m.data.get_temp(vol_id).unwrap_or(1.0));
        let slider = egui::Slider::new(&mut volume, 0.0..=1.0).vertical().show_value(false);
        if ui.add(slider).changed() {
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::SetAudioVolume { source: kind, volume });
            }
        }
        ui.memory_mut(|m| m.data.insert_temp(vol_id, volume));

        // VU meter
        let vu_height = 60.0;
        let vu_width = 12.0;
        let fill_frac = ((current_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let filled_height = vu_height * fill_frac;

        let (rect, _) = ui.allocate_exact_size(egui::vec2(vu_width, vu_height), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);

        let fill_rect = egui::Rect::from_min_max(
            egui::pos2(rect.min.x, rect.max.y - filled_height),
            rect.max,
        );
        let vu_color = if peak_db > -6.0 {
            egui::Color32::RED
        } else if peak_db > -18.0 {
            egui::Color32::YELLOW
        } else {
            egui::Color32::GREEN
        };
        ui.painter().rect_filled(fill_rect, 0.0, vu_color);

        // Mute toggle
        let mute_id = egui::Id::new(("audio_mute", name));
        let mut muted: bool = ui.memory(|m| m.data.get_temp(mute_id).unwrap_or(false));
        let mute_label = if muted { "M" } else { "m" };
        if ui.button(mute_label).clicked() {
            muted = !muted;
            if let Some(ref tx) = state.command_tx {
                let _ = tx.try_send(GstCommand::SetAudioMuted { source: kind, muted });
            }
        }
        ui.memory_mut(|m| m.data.insert_temp(mute_id, muted));
    });
}
```

- [ ] **Step 2: Run `cargo check`**

Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/ui/audio_mixer.rs
git commit -m "feat: implement audio mixer UI with VU meters and controls"
```

---

### Task 8: Update settings window with runtime device enumeration

**Files:**
- Modify: `src/ui/settings_window.rs`

- [ ] **Step 1: Replace hardcoded device lists with `state.available_audio_devices`**

In the `draw_audio` function, change the audio settings to accept `&AppState` instead of just `&mut AudioSettings`. Replace the hardcoded device arrays (`&["Default", "Built-in Microphone", "USB Audio"]`) with `state.available_audio_devices`, filtering by `is_loopback` for system devices vs. mic devices.

Note: The `draw_audio` function signature needs to change from `fn draw_audio(ui: &mut Ui, settings: &mut AudioSettings) -> bool` to `fn draw_audio(ui: &mut Ui, state: &mut AppState) -> bool` so it can access both settings and the device list. Update the call site in the `match category` dispatch accordingly.

- [ ] **Step 2: Send `GstCommand::SetAudioDevice` when user changes device**

When a device is selected in the combo box, send the command via `state.command_tx`.

- [ ] **Step 3: Run `cargo check`**

- [ ] **Step 4: Commit**

```bash
git add src/ui/settings_window.rs
git commit -m "feat: replace hardcoded audio device list with runtime enumeration"
```

---

### Task 9: End-to-end verification

**Files:** None (manual testing)

- [ ] **Step 1: Run the app and verify audio mixer shows levels**

Run: `cargo run`
Expected: Audio mixer panel shows Mic channel with moving VU meter when you speak.

- [ ] **Step 2: Test with BlackHole installed**

If BlackHole is installed, verify the System channel appears and shows levels when system audio is playing.

- [ ] **Step 3: Test recording with audio**

Click Record, speak into mic, stop. Open the MKV file — verify it has two audio tracks (mic + system if BlackHole present).

- [ ] **Step 4: Test streaming with audio**

Start a local RTMP server, Go Live, verify audio is present in the stream via VLC/ffplay.

- [ ] **Step 5: Commit any fixes**

---

### Task 10: Final cleanup

**Files:** Various

- [ ] **Step 1: Run `cargo clippy` and fix warnings**
- [ ] **Step 2: Run `cargo fmt`**
- [ ] **Step 3: Run `cargo test` — all tests pass**
- [ ] **Step 4: Remove unused `#[allow(dead_code)]` annotations**
- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "fix: clippy, fmt, and dead code cleanup for audio capture"
```
