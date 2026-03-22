# Audio Capture Design

## Overview

Add audio capture, encoding, mixing, and metering to Lodestone. Mic capture via `osxaudiosrc`, system audio via `osxaudiosrc` + BlackHole (user-installed virtual audio device). Audio is AAC-encoded and muxed into streams and recordings.

**Platform:** macOS first. Windows later (WASAPI loopback replaces BlackHole requirement).

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Mic capture | `osxaudiosrc` via GStreamer | Standard CoreAudio input, no third-party deps |
| System audio | `osxaudiosrc` + BlackHole | GStreamer has no ScreenCaptureKit audio support. BlackHole is how all Mac streaming apps handle this today. |
| Audio codec | AAC (`avenc_aac` + `aacparse`) | Required for RTMP/FLV streaming. Works in MP4/MKV recording. `aacparse` required between encoder and muxer for proper framing. |
| Streaming mix | GStreamer `audiomixer` element | Handles time-alignment, format conversion, and clipping automatically. More robust than manual Rust mixing. |
| Recording tracks | Separate tracks per source | Mic on track 1, system on track 2. Better for post-production. MKV/MP4 support multi-track. |
| Metering | Peak + RMS via GStreamer `level` element | `level` element provides both out of the box at configurable intervals. |
| Audio caps crate | `gstreamer-audio = "0.23"` | Type-safe audio caps via `AudioCapsBuilder`, consistent with `gstreamer-video` usage. |
| Sample format | S16LE, 48kHz, stereo | Standard for streaming. Consistent across all pipelines. |
| Audio bitrate | 128 kbps default (configurable) | Standard for streaming. Higher quality recording can use 256 kbps. |

## Section 1: Audio Capture Pipelines

Two independent capture pipelines, one per source type:

```
Mic Pipeline:
  osxaudiosrc (device UID from settings)
    → audioconvert
    → audioresample
    → volume (gain control, mute support)
    → level (interval=50ms, peak + RMS)
    → appsink (S16LE, 48kHz, stereo)

System Audio Pipeline:
  osxaudiosrc (BlackHole device UID)
    → audioconvert
    → audioresample
    → volume
    → level (interval=50ms)
    → appsink (S16LE, 48kHz, stereo)
```

- Both produce raw PCM at 48kHz stereo.
- The `level` element emits bus messages with peak/RMS dB — the GStreamer thread reads these and sends them over a `watch` channel to the main thread for the mixer UI.
- Volume/mute applied via the `volume` element before `level`, so metering reflects what the viewer hears.
- Device selection uses the `device` property on `osxaudiosrc` with a **device UID string** (not integer index) — CoreAudio identifies devices by UID.
- Audio caps constructed via `gstreamer_audio::AudioCapsBuilder`:
```rust
let caps = gstreamer_audio::AudioCapsBuilder::new()
    .format(gstreamer_audio::AudioFormat::S16le)
    .rate(48000)
    .channels(2)
    .build();
```

**macOS permissions:** Microphone access requires the "Microphone" TCC permission. The app needs an `NSMicrophoneUsageDescription` in its `Info.plist`. If denied, `osxaudiosrc` will fail — surface `GstError::AudioCaptureFailure` with guidance to System Preferences.

## Section 2: Audio in Encode Pipelines

### Streaming (RTMP — single mixed stereo track)

```
Video: appsrc → videoconvert → vtenc_h264 → h264parse ────────→ flvmux → rtmpsink
Mic:   appsrc (mic PCM) ────→ audiomixer → avenc_aac → aacparse → flvmux
System: appsrc (system PCM) ─┘
```

The `audiomixer` GStreamer element handles time-alignment, sample format conversion, and clipping automatically. Mic and system audio appsrcs feed into `audiomixer`, which produces a single mixed stream for AAC encoding. `aacparse` is required between `avenc_aac` and `flvmux` for proper ADTS framing.

### Recording (MKV/MP4 — multi-track)

```
Video:  appsrc → videoconvert → vtenc_h264 → h264parse ──→ mux → filesink
Mic:    appsrc (mic PCM) → audioconvert → avenc_aac → aacparse → mux
System: appsrc (system PCM) → audioconvert → avenc_aac → aacparse → mux
```

Each audio source gets its own AAC encode chain and track in the container. `matroskamux` and `mp4mux` both support multiple audio pads.

### Pipeline Builder Return Types

The encode pipeline builders change their return types to accommodate audio:

```rust
/// Handles for the streaming pipeline (mixed audio).
pub struct StreamPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub audio_appsrc_mic: AppSrc,
    pub audio_appsrc_system: Option<AppSrc>, // None if no system audio device
}

/// Handles for the recording pipeline (separate audio tracks).
pub struct RecordPipelineHandles {
    pub pipeline: gstreamer::Pipeline,
    pub video_appsrc: AppSrc,
    pub mic_appsrc: AppSrc,
    pub system_appsrc: Option<AppSrc>, // None if no system audio device
}
```

### Audio Encoder Configuration

```rust
pub struct AudioEncoderConfig {
    pub bitrate_kbps: u32,  // default: 128
    pub sample_rate: u32,   // default: 48000
    pub channels: u32,      // default: 2
}
```

### Audio/Video Synchronization

Both video frames and audio buffers use the same `start_time` reference (`Instant::now()` at thread start) for PTS timestamps, ensuring sync.

## Section 3: Channel Architecture Changes

**New channel:**

| Channel | Direction | Type | Mechanism | Behavior |
|---------|-----------|------|-----------|----------|
| Audio Level | GStreamer → UI | `AudioLevelUpdate` | `tokio::sync::watch` | Latest-wins, ~20Hz from `level` element |

**New `GstCommand` variants:**
```rust
SetAudioDevice { source: AudioSourceKind, device_uid: String },
SetAudioVolume { source: AudioSourceKind, volume: f32 },
SetAudioMuted { source: AudioSourceKind, muted: bool },
```

**New types:**
```rust
#[derive(Debug, Clone, Copy)]
pub enum AudioSourceKind {
    Mic,
    System,
}

#[derive(Debug, Clone, Default)]
pub struct AudioLevelUpdate {
    pub mic: Option<AudioLevels>,
    pub system: Option<AudioLevels>,
}

#[derive(Debug, Clone)]
pub struct AudioLevels {
    pub peak_db: f32,
    pub rms_db: f32,
}
```

**State migration:** The existing `AudioLevel` struct (keyed on `SourceId`) and `audio_levels: Vec<AudioLevel>` in `AppState` are replaced by `audio_levels: AudioLevelUpdate`. The old type is unused placeholder code and can be removed.

## Section 4: Audio Mixer UI

The placeholder in `audio_mixer.rs` is replaced with a real mixer showing two fixed channels: Mic and System Audio.

Each channel strip:
- Source name label ("Mic", "System")
- Vertical volume fader (0.0–1.0, sends `GstCommand::SetAudioVolume`)
- VU meter bar (peak + RMS, colored green/yellow/red at -18dB/-6dB thresholds)
- Mute toggle button (sends `GstCommand::SetAudioMuted`)

**Device selection** stays in the Settings window audio tab. The hardcoded device list is replaced with runtime enumeration via GStreamer's `DeviceMonitor`.

**BlackHole guidance:** If no loopback/virtual audio device is detected, the System Audio channel shows "Install BlackHole for system audio capture". Non-blocking — mic works independently.

## Section 5: GStreamer Thread Changes

New fields on `GstThread`:

```rust
mic_pipeline: Option<gstreamer::Pipeline>,
mic_appsink: Option<AppSink>,
system_pipeline: Option<gstreamer::Pipeline>,
system_appsink: Option<AppSink>,
mic_volume: f32,
mic_muted: bool,
system_volume: f32,
system_muted: bool,
audio_level_tx: watch::Sender<AudioLevelUpdate>,
```

The `stream_appsrc` and `record_appsrc` fields are replaced by the new handle structs:
```rust
stream_handles: Option<StreamPipelineHandles>,
record_handles: Option<RecordPipelineHandles>,
```

**Run loop additions:**
1. Pull audio samples from mic appsink
2. Pull audio samples from system appsink
3. If streaming: push mic and system audio to their respective stream appsrcs (the `audiomixer` element in the pipeline handles mixing)
4. If recording: push mic and system audio separately to their respective record appsrcs
5. Check pipeline bus for `level` element messages, update `audio_level_tx`

**Pipeline shutdown (EOS handling):**
When stopping a stream or recording, EOS must be sent on **all** appsrcs (video + audio) before waiting for the pipeline-level EOS on the bus. Otherwise the muxer will hang waiting for audio EOS and the 2-second timeout will fire, potentially truncating the file.

**Lifecycle:**
- Mic pipeline starts on app launch (like video capture)
- System pipeline starts only if a BlackHole/virtual device is detected
- Both capture continuously for metering, independent of stream/record state
- Volume/mute changes update the `volume` element property in real-time (no pipeline restart)
- If a device disappears mid-session (USB mic unplugged), send `GstError::AudioCaptureFailure` and stop that audio pipeline. Metering shows silence. The other audio source and video continue unaffected.

## Section 6: Module Structure

```
src/gstreamer/
├── mod.rs          — add re-exports for new audio types
├── capture.rs      — add build_audio_capture_pipeline()
├── encode.rs       — new return types (StreamPipelineHandles, RecordPipelineHandles), audio appsrc + audiomixer/aacparse elements
├── commands.rs     — add AudioSourceKind, AudioEncoderConfig, audio GstCommand variants
├── types.rs        — add AudioLevelUpdate, AudioLevels, AudioSourceKind
├── thread.rs       — add audio pipeline management, level reporting, multi-appsrc EOS
├── error.rs        — add AudioCaptureFailure variant
└── devices.rs      — NEW: device enumeration via GStreamer DeviceMonitor
```

**`devices.rs`:**
- Wraps GStreamer's `DeviceMonitor` to list audio input devices
- Returns `Vec<AudioDevice { uid: String, name: String, is_loopback: bool }>`
- `is_loopback` detected by matching device name against known virtual devices (BlackHole, Soundflower, Loopback)
- Called on GStreamer thread startup, results sent to main thread
- Monitors for device changes (connect/disconnect) and re-enumerates

**Other modified files:**
- `Cargo.toml` — add `gstreamer-audio = "0.23"`
- `src/state.rs` — replace `Vec<AudioLevel>` with `AudioLevelUpdate`, add `available_audio_devices` field
- `src/ui/audio_mixer.rs` — full mixer UI replacing placeholder
- `src/ui/settings_window.rs` — runtime device enumeration replaces hardcoded list

## Future Extensions

Out of scope but accommodated by the design:
- **ScreenCaptureKit audio** — bypass BlackHole requirement when GStreamer or native bindings add support
- **Opus recording** — swap `avenc_aac` for `opusenc` in the record pipeline
- **Per-source audio filters** — noise gate, compressor via GStreamer audio filter elements
- **Audio monitoring** — route audio to local output device for monitoring
- **Windows support** — swap `osxaudiosrc` for `wasapisrc` with loopback support (no BlackHole needed)
