# GStreamer OBS Integration Design

## Overview

Replace the mock OBS backend with a real capture/encode/stream/record pipeline powered by GStreamer. Lodestone owns composition via wgpu; GStreamer handles capture input and encode/output. Video-only initially — audio is a fast follow.

**Platform:** macOS first (VideoToolbox H.264), Windows later (NVENC).

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Capture/encode backend | GStreamer (`gstreamer-rs`) | Cross-platform, mature, hardware-accelerated. libobs-rs has no macOS support. |
| Composition | wgpu (existing) | Already built. GStreamer provides raw frames in, takes composited frames out. |
| Process model | In-process, dedicated thread | 30-60fps frame throughput makes IPC impractical. GStreamer is stable. |
| Thread communication | Multiple typed channels | Separate frame, command, stats, error channels. High-frequency frame data doesn't compete with control messages. |
| Streaming protocol | RTMP | Covers Twitch, YouTube, all major platforms. |
| Recording formats | MKV (default), MP4 | MKV is crash-safe. MP4 for compatibility (note: MP4 files will be corrupt if process crashes mid-recording since moov atom is written at finalization). |
| Encoder | VideoToolbox H.264 (`vtenc_h264`) | Hardware-accelerated, universal on Mac, all platforms accept H.264. |
| Audio | Deferred | Video pipeline is the harder problem. Audio plugs into GStreamer cleanly once video works. |

## Architecture

```
winit event loop (main thread)
  ├─ Polls FrameRx → uploads to PreviewRenderer
  ├─ Polls StatsRx → updates AppState.stream_status
  ├─ Polls ErrorRx → surfaces errors in UI
  └─ Sends GstCommand via CommandTx on user actions

GStreamer thread (std::thread::spawn)
  ├─ Capture pipeline: avfvideosrc → videoconvert → videoscale → videorate → appsink
  ├─ Encode pipeline (built on demand): appsrc → videoconvert → vtenc_h264 → h264parse
  │     ├─→ flvmux → rtmpsink        (streaming)
  │     └─→ matroskamux/mp4mux → filesink (recording)
  ├─ Listens on CommandRx
  ├─ Sends frames on FrameTx
  ├─ Sends stats on StatsTx
  └─ Sends errors on ErrorTx
```

## Section 1: Capture Pipeline

GStreamer captures the screen and delivers raw RGBA frames to Lodestone.

```
avfvideosrc (ScreenCaptureKit) → videoconvert → videoscale → videorate → appsink
```

- **`avfvideosrc`** — macOS capture element using ScreenCaptureKit under the hood. Requires `capture-screen=true` property (defaults to camera otherwise) and `screen-index` property for multi-monitor selection.
- **`videoconvert`** — converts to RGBA for wgpu texture upload.
- **`videoscale` + `videorate`** — normalizes resolution and framerate.
- **`appsink`** — pulls frames out of GStreamer into Rust, sends over a dedicated frame channel.

**macOS permissions:** Screen capture requires the "Screen Recording" TCC permission (macOS 13+). If denied, `avfvideosrc` produces black frames or fails. The app should detect this and surface a message guiding the user to System Preferences → Privacy & Security → Screen Recording.

Source types are abstracted via an enum:

```rust
pub enum CaptureSourceConfig {
    Screen { screen_index: u32 },
    Window { window_id: u64 },     // future
    Camera { device_index: u32 },  // future
}
```

Each variant maps to a different GStreamer source element. The rest of the pipeline stays the same. Adding a new source type = adding a variant + a function returning the right element.

## Section 2: Composition Layer

The existing wgpu pipeline handles composition. Capture frames flow in, composited frames flow out.

- **Source textures**: Each active source gets a wgpu texture. Frames from the capture channel are uploaded via `queue.write_texture()`.
- **Scene rendering**: A render pass composites all visible sources according to the scene's `Transform` data (position, size, layering order). Reuses the existing `Scene`/`Source` model.
- **Dual output**: The compositor renders to a texture that serves both the preview panel and (when streaming/recording) a GPU readback buffer.
- **Readback**: When encoding is active, the compositor renders to a texture with `COPY_SRC | RENDER_ATTACHMENT` usage. A staging buffer with `COPY_DST | MAP_READ` usage receives the data via `encoder.copy_texture_to_buffer()`. Double-buffered staging buffers prevent GPU pipeline stalls — one buffer is mapped for CPU read while the other receives the next frame. `map_async` with `device.poll(Maintain::Wait)` ensures the readback completes before pushing to the encode pipeline.

The readback only runs when streaming or recording is active — no GPU→CPU copy cost during idle preview.

## Section 3: Encode & Output Pipeline

Composited frames are pushed into a second GStreamer pipeline for encoding and delivery.

```
appsrc → videoconvert → vtenc_h264 → h264parse
   ├─→ tee → flvmux → rtmpsink              (streaming)
   └─→ tee → matroskamux/mp4mux → filesink  (recording)
```

- **`appsrc`** — Lodestone pushes composited frames into GStreamer. Caps: `video/x-raw,format=RGBA,width=W,height=H,framerate=F/1`. Stream type: stream (not random-access).
- **`videoconvert`** — required, not optional. Converts RGBA (from GPU readback) to NV12/I420 as required by `vtenc_h264`.
- **`vtenc_h264`** — hardware H.264 encoding via VideoToolbox. `bitrate` property is in kbit/s, maps directly from `EncoderConfig.bitrate_kbps`.
- **`tee` element** — when both streaming and recording are active, the encoded stream splits after `h264parse` to per-branch muxers.
- **Streaming**: `flvmux` → `rtmpsink` for RTMP delivery.
- **Recording**: `matroskamux` → `filesink` for MKV (default), or `mp4mux` → `filesink` for MP4.

The pipeline is built dynamically:
- Preview only → no encode pipeline running.
- Stream only → `appsrc → videoconvert → vtenc_h264 → h264parse → flvmux → rtmpsink`.
- Record only → `appsrc → videoconvert → vtenc_h264 → h264parse → mux → filesink`.
- Stream + record → `appsrc → videoconvert → vtenc_h264 → h264parse → tee → [flvmux → rtmpsink, mux → filesink]`.

Encoder settings (`bitrate_kbps`, `width`, `height`, `fps`) map from the existing `EncoderConfig` struct to `vtenc_h264` element properties.

## Section 4: Channel Architecture

Four typed channels between the GStreamer thread and the main app:

```rust
pub enum GstCommand {
    SetCaptureSource(CaptureSourceConfig),
    StartStream(StreamConfig),
    StopStream,
    StartRecording { path: PathBuf, format: RecordingFormat },
    StopRecording,
    UpdateEncoder(EncoderConfig),
    Shutdown,
}

pub enum RecordingFormat {
    Mkv,
    Mp4,
}

pub enum GstError {
    CaptureFailure { message: String },
    EncodeFailure { message: String },
    StreamConnectionLost { message: String },
    PipelineStateChange { from: String, to: String, message: String },
    PermissionDenied { message: String },
}
```

| Channel | Direction | Bounded | Behavior |
|---------|-----------|---------|----------|
| Command | UI → GStreamer | 16 | Plenty of headroom for UI commands |
| Frame | GStreamer → UI | 2 | `try_send`, drop newest on full (renderer keeps displaying last received frame) |
| Stats | GStreamer → UI | 1 | Latest-wins via `tokio::sync::watch` |
| Error | GStreamer → UI | unbounded | Errors are never dropped |

The main loop polls frame and stats receivers each tick. Errors are checked and surfaced to the UI.

## Section 5: Thread & Lifecycle Management

**Startup:**
1. `gst::init()` on the GStreamer thread.
2. Build capture pipeline with default screen source.
3. Frames start flowing to the preview immediately.
4. Encode pipeline is not built until user hits "Go Live" or "Record."

**Shutdown:**
1. Main thread sends `GstCommand::Shutdown`.
2. GStreamer thread sets all pipelines to `Null` state, drains buffers.
3. Thread joins cleanly via `JoinHandle`.
4. If recording was active, the file is properly finalized (mux trailer written).

**Error recovery:**
- Capture pipeline error (e.g., screen disconnected): send `GstError::CaptureFailure`, attempt restart with default source.
- Encode pipeline error (e.g., RTMP connection lost): send `GstError::StreamConnectionLost`, stop encode pipeline, keep capture running.
- Permission denied: send `GstError::PermissionDenied`, UI shows guidance to System Preferences.
- UI shows errors in stream controls panel — no modal dialogs.

**Replacing the mock:**
- `MockObsEngine` and `mock_driver` are replaced by the real GStreamer thread.
- The `ObsEngine` trait is retired — the channel-based architecture supersedes it.
- `Scene`/`Source`/`Transform` types move to a top-level `scene.rs`.
- UI panels send commands via `CommandTx` instead of calling trait methods.
- **Scene/source management stays on the main thread.** The `AppState` owns scenes and sources. The compositor reads scene state to determine what to render and how. GStreamer only knows about capture sources — it has no concept of the scene graph. `GstCommand::SetCaptureSource` tells GStreamer which screen/window/camera to capture; the compositor decides where/how to render it based on scene transforms.
- **Audio mixer panel**: temporarily shows static/disabled state while audio is deferred. The panel remains in the layout but displays a "No audio sources" placeholder.

## Section 6: Module Structure & Type Migration

Types currently in `src/obs/` are relocated:

| Type | Current location | New location |
|------|-----------------|--------------|
| `Scene`, `Source`, `SourceType`, `Transform`, `SceneId`, `SourceId` | `obs/scene.rs` | `src/scene.rs` |
| `RgbaFrame`, `ObsStats` | `obs/mod.rs` | `src/gstreamer/types.rs` |
| `StreamConfig`, `StreamDestination` | `obs/output.rs` | `src/gstreamer/commands.rs` |
| `EncoderConfig` | `obs/encoder.rs` | `src/gstreamer/commands.rs` |

```
src/
├── gstreamer/
│   ├── mod.rs              — public API: spawn_gstreamer_thread() → (GstChannels, JoinHandle)
│   ├── capture.rs          — capture pipeline builder (avfvideosrc → appsink)
│   ├── encode.rs           — encode pipeline builder (appsrc → vtenc → outputs)
│   ├── commands.rs         — GstCommand, RecordingFormat, StreamConfig, EncoderConfig, channel constructors
│   ├── types.rs            — RgbaFrame, ObsStats, GstChannels
│   └── error.rs            — GstError enum, GStreamer bus message handling
├── renderer/
│   ├── preview.rs          — unchanged, receives RgbaFrame from frame channel
│   ├── compositor.rs       — NEW: multi-source composition + double-buffered readback
│   └── ...
├── scene.rs                — Scene/Source/Transform/SceneId/SourceId types
├── state.rs                — AppState gains: active_errors: Vec<GstError>, recording_status
├── mock_driver.rs          — DELETED
└── main.rs                 — spawns GStreamer thread in AppManager::new(), stores GstChannels + JoinHandle
```

- **`gstreamer/` is self-contained** — all GStreamer dependencies isolated here. Nothing outside imports `gstreamer-rs`.
- **`commands.rs`** owns the channel types and constructors via a `GstChannels` struct.
- **`compositor.rs`** handles multi-source texture management and GPU readback. Separate from `preview.rs`.
- **`obs/` is removed.** `mock_driver.rs` is deleted.

```rust
/// Returned by spawn_gstreamer_thread()
pub struct GstChannels {
    pub command_tx: mpsc::Sender<GstCommand>,
    pub frame_rx: mpsc::Receiver<RgbaFrame>,
    pub stats_rx: watch::Receiver<ObsStats>,
    pub error_rx: mpsc::UnboundedReceiver<GstError>,
}
```

## GStreamer Distribution

GStreamer must be available at runtime. Strategy per platform:
- **macOS (development)**: Homebrew (`brew install gstreamer`). `pkg-config` used for build-time linking.
- **macOS (distribution)**: Bundle `GStreamer.framework` inside the `.app` bundle's `Frameworks/` directory.
- **Windows (future)**: Bundle GStreamer DLLs alongside the executable, or use the GStreamer MSVC installer.

## Future Extensions

These are explicitly out of scope for the initial implementation but the design accommodates them:

- **Audio capture and mixing** — add audio `appsink`/`appsrc` elements to existing pipelines.
- **Window and camera sources** — add `CaptureSourceConfig` variants mapping to GStreamer elements.
- **HEVC encoding** — swap `vtenc_h264` for `vtenc_h265`, change muxer. Note: RTMP/FLV does not support HEVC without Enhanced RTMP.
- **SRT streaming** — swap `rtmpsink` for `srtsink`.
- **Windows support** — swap `avfvideosrc` for `d3d11screencapturesrc`, `vtenc_h264` for `nvh264enc`.
- **Scene transitions** — compositor renders both scenes, crossfades via shader.
- **GPU-side colorspace conversion** — compute shader for RGBA→NV12 before readback to halve bandwidth.
