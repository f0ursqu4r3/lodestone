# Recording & Streaming Improvements — Design Spec

**Date:** 2026-03-26
**Status:** Draft
**Scope:** Recording pipeline, streaming pipeline, encoder detection, settings UI, toolbar

## Overview

Lodestone has working recording and streaming pipelines but they suffer from stale configuration, hardcoded values, and disconnected UI. This spec covers the changes needed to make recording and streaming production-ready: proper encoder detection, separate quality settings for stream vs record, filename templates, destination wiring, and settings UI.

Everything in this spec is MVP. There is no "later phase" tier.

## 1. Data Model

### 1.1 Encoder Types and Detection

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

/// An encoder detected at startup.
#[derive(Debug, Clone)]
pub struct AvailableEncoder {
    pub encoder_type: EncoderType,
    pub display_name: String,      // e.g. "VideoToolbox (Hardware)"
    pub is_hardware: bool,
    pub is_recommended: bool,      // best auto-detected choice
}
```

Detection runs once on the GStreamer thread at startup. For each known element name (`vtenc_h264`, `x264enc`, `nvh264enc`, `amfh264enc`, `qsvh264enc`), attempt `ElementFactory::make`. If it succeeds, add to the available list.

Auto-select priority: VideoToolbox > NVENC > AMF > QSV > x264. The first hardware encoder found is marked `is_recommended = true`. If no hardware encoder exists, x264 is recommended.

Results sent to UI via a new `watch` channel (same pattern as `devices_tx`).

### 1.2 Quality Presets

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityPreset {
    Low,      // 2500 kbps
    Medium,   // 4500 kbps
    High,     // 8000 kbps
    Custom,   // user-specified bitrate
}
```

`QualityPreset` resolves to a bitrate via a simple match. When `Custom` is selected, the user-specified `bitrate_kbps` field is used instead.

### 1.3 EncoderConfig Changes

```rust
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_type: EncoderType,  // NEW
}
```

Width and height always come from `VideoSettings.output_resolution` at the moment encoding starts. They are not stored in `StreamSettings` or `RecordSettings`.

### 1.4 Settings Changes

**StreamDestination** (unchanged — keeps existing data-carrying variant):

```rust
pub enum StreamDestination {
    Twitch,
    YouTube,
    CustomRtmp { url: String },  // existing shape, carries the full RTMP URL
}
```

**StreamSettings** (updated):

```rust
pub struct StreamSettings {
    pub stream_key: String,
    pub destination: StreamDestination,   // CustomRtmp { url } carries the URL
    pub encoder: EncoderType,             // was String
    pub quality_preset: QualityPreset,    // NEW
    pub bitrate_kbps: u32,               // used when quality_preset is Custom
    pub fps: u32,
}
```

Fields removed: `width`, `height` (now derived from `VideoSettings.output_resolution`).

**RecordSettings** (new struct):

```rust
pub struct RecordSettings {
    pub format: RecordingFormat,
    pub output_folder: PathBuf,
    pub filename_template: String,        // e.g. "{date}_{time}_{scene}"
    pub encoder: EncoderType,
    pub quality_preset: QualityPreset,
    pub bitrate_kbps: u32,               // used when quality_preset is Custom
    pub fps: u32,
}
```

Defaults: format = MKV, output_folder = `dirs::video_dir()`, filename_template = `{date}_{time}_{scene}`, quality = High (8000 kbps), fps = 30.

**AppSettings** gains `pub record: RecordSettings`.

**AppState** gains:
- `pub available_encoders: Vec<AvailableEncoder>` — populated from GStreamer thread at startup
- `pub recording_started_at: Option<std::time::Instant>` — for toolbar timer

## 2. GStreamer Command Changes

### 2.1 StartStream

Before:
```rust
GstCommand::StartStream(StreamConfig)  // StreamConfig { destination, stream_key }
```

After:
```rust
GstCommand::StartStream {
    destination: StreamDestination,  // CustomRtmp { url } carries URL inline
    stream_key: String,              // used for Twitch/YouTube, empty for CustomRtmp
    encoder_config: EncoderConfig,
}
```

The `StreamConfig` struct is removed. All config is passed directly in the command.

### 2.2 StartRecording

Before:
```rust
GstCommand::StartRecording { path: PathBuf, format: RecordingFormat }
```

After:
```rust
GstCommand::StartRecording {
    path: PathBuf,
    format: RecordingFormat,
    encoder_config: EncoderConfig,
}
```

### 2.3 Removed Commands

`UpdateEncoder(EncoderConfig)` is removed. There is no stale encoder config on the GStreamer thread — each start command carries its own config.

### 2.4 New Channel

`encoders_tx: watch::Sender<Vec<AvailableEncoder>>` — sent once at GStreamer thread startup after detection completes.

## 3. Encode Pipeline Changes

### 3.1 Encoder Dispatch

`build_encode_chain` currently hardcodes `vtenc_h264` with an `x264enc` fallback. Replace with a dispatch based on `EncoderType`:

```rust
fn make_encoder(encoder_type: EncoderType, bitrate_kbps: u32) -> Result<Element> {
    match encoder_type {
        EncoderType::H264VideoToolbox => {
            ElementFactory::make("vtenc_h264")
                .property("bitrate", bitrate_kbps)
                .property("realtime", true)
                .property("allow-frame-reordering", false)
                .build()
        }
        EncoderType::H264x264 => {
            ElementFactory::make("x264enc")
                .property("bitrate", bitrate_kbps)
                .property("tune", 0x04u32)  // zerolatency
                .build()
        }
        EncoderType::H264Nvenc => {
            ElementFactory::make("nvh264enc")
                .property("bitrate", bitrate_kbps)
                .property("preset", 3u32)   // low-latency
                .build()
        }
        EncoderType::H264Amf => {
            ElementFactory::make("amfh264enc")
                .property("bitrate", bitrate_kbps)
                .build()
        }
        EncoderType::H264Qsv => {
            ElementFactory::make("qsvh264enc")
                .property("bitrate", bitrate_kbps)
                .build()
        }
    }
}
```

### 3.2 Independent Stream/Record Pipelines

Stream and record pipelines already run independently. With per-command `EncoderConfig`, they can now use different encoders and quality levels simultaneously (e.g. hardware encode at 4500 kbps for stream, software encode at 8000 kbps for local recording).

## 4. RTMP URL Assembly

`StreamDestination` already has `rtmp_url()` returning base URLs. The full URL assembly:

- **Twitch**: `rtmp://live.twitch.tv/app/{stream_key}`
- **YouTube**: `rtmp://a.rtmp.youtube.com/live2/{stream_key}`
- **Custom RTMP**: `destination.rtmp_url()` returns the URL from `CustomRtmp { url }` as-is (user provides full URL)

### 4.1 Validation

Before sending `StartStream`:
- **Twitch/YouTube**: `stream_key` must be non-empty.
- **Custom RTMP**: `url` inside `CustomRtmp { url }` must be non-empty and start with `rtmp://` or `rtmps://`.

Validation failures surface as `GstError` entries in `AppState.active_errors` and the Go Live button does not change state.

## 5. Recording Output

### 5.1 Filename Templates

Supported tokens:
- `{date}` → `2026-03-26` (local date, ISO 8601)
- `{time}` → `18-30-45` (local time, dash-separated for filesystem safety)
- `{scene}` → active scene name (sanitized: non-alphanumeric chars replaced with `_`)
- `{n}` → auto-incrementing number, starts at 1 per app session

Default template: `{date}_{time}_{scene}`
Example: `2026-03-26_18-30-45_Gaming.mkv`

The file extension comes from `RecordSettings.format`, not the template.

### 5.2 Output Folder Resolution

1. `RecordSettings.output_folder` if set and exists
2. `dirs::video_dir()` fallback
3. Home directory fallback

## 6. Toolbar Changes

### 6.1 Recording Timer

When `recording_started_at` is `Some`, the toolbar REC indicator shows elapsed time:

```
[REC 00:05:32]
```

Red pulsing dot (existing animation) + elapsed time in `HH:MM:SS`. Computed via `recording_started_at.elapsed()` each frame. `recording_started_at` is set when `StartRecording` is sent and cleared on `StopRecording`.

### 6.2 Stream Destination from Settings

The Go Live button reads `state.settings.stream.destination` instead of hardcoding `StreamDestination::Twitch`. Assembles the full command from settings at click time.

### 6.3 Validation Feedback

If validation fails (no stream key, invalid RTMP URL, invalid output folder):
- The button does not change state
- An error is pushed to `AppState.active_errors`
- The existing error display mechanism surfaces it in the UI

### 6.4 Stream Stats

The existing stream status display (uptime, bitrate, dropped frames) continues to work as-is. The GStreamer thread already tracks `StreamStatus::Live { uptime_secs, bitrate_kbps, dropped_frames }`. Future work can populate these from actual GStreamer pipeline statistics.

## 7. Settings UI

### 7.1 Stream Settings Tab

Updated layout:
1. **Destination** — dropdown: Twitch, YouTube, Custom RTMP
2. **Stream Key** — password-masked input, shown for Twitch/YouTube
3. **RTMP URL** — text input, shown only when Custom RTMP selected (edits `CustomRtmp { url }`)
4. Separator
5. **Encoder** — dropdown showing only detected encoders, recommended one marked with "— Recommended" suffix
6. **Quality** — toggle buttons: Low, Medium, High, Custom. Custom reveals a bitrate `DragValue` input
7. **FPS** — toggle buttons: 24, 30, 60

### 7.2 Recording Settings Tab (New)

New tab in the settings window:
1. **Format** — toggle buttons: MKV, MP4. Hint text: "MKV is crash-safe"
2. **Output Folder** — path display + Browse button (native file dialog)
3. **Filename Template** — text input with live preview below
4. Separator
5. **Encoder** — same dropdown pattern as stream tab
6. **Quality** — same toggle pattern (Low, Medium, High, Custom), defaults to High
7. **FPS** — same toggle pattern

### 7.3 Encoder Dropdown Behavior

The dropdown only lists encoders from `AppState.available_encoders`. Each entry shows:
- Display name (e.g. "VideoToolbox (Hardware)")
- "— Recommended" appended to the auto-detected best choice

On first launch (no saved preference), the recommended encoder is auto-selected for both stream and record settings.

## 8. Error Handling

- Encoder detection failure for a specific encoder is silently skipped (not all encoders exist on all systems).
- If zero encoders are detected (shouldn't happen — x264 is always available via GStreamer), surface a startup error.
- Pipeline creation failures surface via the existing `GstError::EncodeFailure` path.
- RTMP connection failures surface via GStreamer bus error messages forwarded through `error_tx`.

## 9. Backwards Compatibility

Settings are TOML with `#[serde(default)]`. New fields get defaults:
- `RecordSettings` gets default values when missing from existing TOML files
- `StreamSettings.encoder` changes from `String` to `EncoderType` — serde deserialization of old string values falls back to default (`H264VideoToolbox` or auto-detected)
- `StreamSettings.width`/`height` removal is safe — they were not read by the compositor anyway

## 10. Testing

- **Encoder detection**: unit test that `enumerate_encoders()` returns at least x264
- **Quality presets**: unit test that each preset maps to expected bitrate
- **Filename templates**: unit test token substitution with various templates
- **RTMP URL assembly**: unit test for each destination type
- **Validation**: unit test that empty stream key / invalid RTMP URL returns error
- **Settings roundtrip**: existing `save_and_load_roundtrip` test extended for new fields
- **Pipeline construction**: existing `build_stream/record_with_audio` tests updated for new `EncoderConfig` with `encoder_type`
