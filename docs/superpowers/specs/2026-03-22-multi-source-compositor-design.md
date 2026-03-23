# Multi-Source Compositor Design

## Overview

A GPU-based compositor that blends multiple capture sources into a single scene canvas using wgpu render pipelines. Each source runs its own GStreamer capture pipeline. The composited result feeds both the preview display and the encode path (via GPU readback).

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Composition engine | GPU (wgpu) | Already have wgpu renderer; unlocks shaders, blend modes, overlays |
| Multi-capture | Separate pipeline per source | Independent lifecycles, simple, matches existing pattern |
| Canvas resolution | Fixed = output resolution | Simple, no extra scaling pass. Virtual canvas deferred to future |
| Source transforms | Position + Size + Opacity | Opacity is trivial in shader, unlocks overlays. Crop deferred |
| Readback strategy | Sync on render thread | ~1-2ms for 1080p RGBA. Async readback thread is a future optimization |
| Z-ordering | Vec index in scene.sources | Bottom-first draw order. Reorder by moving vec elements |

## Architecture

```
┌─────────────────── GStreamer Thread ───────────────────┐
│                                                        │
│  captures: HashMap<SourceId, CaptureHandle>            │
│                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │ Display  │  │ Webcam   │  │ Display  │  ...       │
│  │ capture  │  │ capture  │  │ capture  │            │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘            │
│       │              │              │                  │
│       └──────────────┼──────────────┘                  │
│                      │                                 │
│    latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>
│                      │                                 │
└──────────────────────┼─────────────────────────────────┘
                       │ shared memory (latest frame per source)
                       ▼
┌─────────────────── Render Thread ──────────────────────┐
│                                                        │
│  Compositor                                            │
│    source_layers: HashMap<SourceId, SourceLayer>       │
│    canvas_texture: wgpu::Texture (output resolution)   │
│    compositor_pipeline: wgpu::RenderPipeline           │
│                                                        │
│    Per frame:                                          │
│      1. Lock latest_frames, drain into source textures │
│      2. Begin render pass on canvas_texture            │
│      3. For each source in scene vec order:            │
│         - Bind source texture                          │
│         - Push transform + opacity uniforms            │
│         - Draw textured quad                           │
│      4. End render pass                                │
│                                                        │
│    canvas_texture feeds:                               │
│      → PreviewRenderer (samples canvas for display)    │
│      → Readback (copy_texture_to_buffer → submit →    │
│         poll(Wait) → map → RgbaFrame → encode channel) │
│                                                        │
│  composited_frame_tx ──→ GStreamer Thread encode appsrc│
└────────────────────────────────────────────────────────┘
```

## Components

### New: `src/renderer/compositor.rs`

The GPU compositor. Owns all composition resources and logic.

**Types:**

```rust
/// Per-source GPU state
struct SourceLayer {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),  // native frame dimensions
}

/// Transform uniforms pushed per draw call.
/// The `compose()` method converts pixel-space Transform values
/// to normalized 0.0..1.0 coordinates by dividing by canvas dimensions.
#[repr(C)]
struct SourceUniforms {
    /// Normalized position and size on canvas (0.0..1.0)
    rect: [f32; 4],  // x, y, width, height
    /// Source opacity (0.0 = transparent, 1.0 = opaque)
    opacity: f32,
    _padding: [f32; 3],
}

pub struct Compositor {
    canvas_texture: wgpu::Texture,
    canvas_view: wgpu::TextureView,
    canvas_width: u32,
    canvas_height: u32,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    uniform_buffer: wgpu::Buffer,
    source_layers: HashMap<SourceId, SourceLayer>,
    readback_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
}
```

**Key methods:**

- `new(device, canvas_width, canvas_height)` — create canvas texture, pipeline, readback buffer
- `upload_frame(device, queue, source_id, frame: &RgbaFrame)` — upload frame to source texture, creating/resizing texture if needed
- `remove_source(source_id)` — drop source texture and bind group
- `compose(device, queue, encoder, sources: &[&Source])` — takes a pre-resolved, ordered slice of visible `Source` references. The caller resolves `SourceId`s from the active scene against `SceneCollection.sources` and filters to visible sources. `compose()` converts each `Source.transform` (pixel-space) to normalized `SourceUniforms.rect` by dividing x/width by `canvas_width` and y/height by `canvas_height`. Sources without a `SourceLayer` (no frame received yet) are skipped.
- `readback(device, queue) -> RgbaFrame` — copies canvas to readback buffer via `encoder.copy_texture_to_buffer()`, submits with `queue.submit()`, blocks with `device.poll(Maintain::Wait)`, then maps the buffer with `slice.get_mapped_range()` and copies to an `RgbaFrame`. This is a synchronous blocking call (~1-2ms for 1080p).

**Shader (`compositor.wgsl`):**

Vertex shader takes a fullscreen quad and applies the source transform (position + scale on canvas). Fragment shader samples the source texture and multiplies by opacity uniform. Blend state is alpha-over (standard Porter-Duff source-over).

### Modified: `src/gstreamer/thread.rs`

Multi-pipeline capture management.

**Changes:**

- `capture_pipeline: Option<CaptureHandle>` → `captures: HashMap<SourceId, CaptureHandle>`
- New `CaptureHandle` struct bundles a pipeline + appsink per source
- Run loop iterates `captures.values()` sequentially, calling `try_pull_sample` with 0ms timeout (non-blocking) on each appsink — same pattern as the existing single-capture pull
- Latest frame per source is written to `latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>` (shared with render thread) — replaces the bounded mpsc channel. This is a "latest frame wins" design: the GStreamer thread overwrites, the render thread reads and clears.
- New `composited_frame_rx: tokio::sync::mpsc::Receiver<RgbaFrame>` — receives composited frames from the render thread for encoding. The run loop checks this channel and pushes to active encode appsrcs (replacing the current direct capture→encode push).

### Modified: `src/gstreamer/commands.rs`

New commands for source lifecycle:

```rust
enum GstCommand {
    // Existing
    SetCaptureSource(CaptureSourceConfig),  // deprecated, kept for compat
    StartStream(StreamConfig),
    StopStream,
    StartRecording { path, format },
    StopRecording,
    // ...

    // New
    AddCaptureSource { source_id: SourceId, config: CaptureSourceConfig },
    RemoveCaptureSource { source_id: SourceId },
}
```

Channel changes:

```rust
// Before
frame_tx: tokio::sync::mpsc::Sender<RgbaFrame>

// After (shared latest-frame map replaces the frame channel)
latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>

// New: composited frame back-channel for encoding
composited_frame_tx: tokio::sync::mpsc::Sender<RgbaFrame>  // render → gstreamer
composited_frame_rx: tokio::sync::mpsc::Receiver<RgbaFrame> // capacity 2, lossy
```

### Modified: `src/renderer/preview.rs`

Simplified. No longer receives raw frames or manages its own texture. Instead, the preview callback samples the compositor's `canvas_texture` directly via a shared bind group.

Changes:
- Remove `upload_frame()` and internal texture
- Accept `&wgpu::BindGroup` from compositor's canvas texture
- `paint()` draws a fullscreen quad sampling the canvas (same letterbox logic)

### Modified: `src/scene.rs`

Add opacity to Source (with `serde(default)` for backwards compat):

```rust
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub properties: SourceProperties,
    pub transform: Transform,          // existing type name
    #[serde(default = "default_opacity")]
    pub opacity: f32,                  // NEW: 0.0..=1.0, default 1.0
    pub visible: bool,
    pub muted: bool,
    pub volume: f32,
}

fn default_opacity() -> f32 { 1.0 }
```

`Transform` stays as-is (pixel-space `x, y, width, height`). The compositor converts to normalized coords.

Add source reordering methods to Scene:

```rust
impl Scene {
    pub fn move_source_up(&mut self, source_id: SourceId) { ... }
    pub fn move_source_down(&mut self, source_id: SourceId) { ... }
}
```

### Modified: `src/ui/scene_editor.rs`

Support multiple sources per scene:

- List all sources in the scene (not just the first)
- Add/remove source buttons per scene
- Per-source controls: transform fields, opacity slider, visibility toggle
- Source reordering (move up/move down buttons)
- Selecting a source highlights it in the preview (future: interactive transform handles)

### Modified: `src/state.rs`

- Remove `frame_rx` (replaced by `latest_frames: Arc<Mutex<HashMap<SourceId, RgbaFrame>>>`)
- Add `composited_frame_tx: tokio::sync::mpsc::Sender<RgbaFrame>` for encode path
- Remove `preview_width`/`preview_height` (compositor owns canvas dimensions)

### Modified: `src/window.rs` / `src/main.rs`

- Create `Compositor` alongside `SharedGpuState`
- Each render frame: lock `latest_frames`, upload any new frames via `compositor.upload_frame()`, clear the map
- Resolve active scene sources: look up `SourceId`s from active scene in `SceneCollection.sources`, filter visible, pass ordered slice to `compositor.compose()`
- After compose, if streaming/recording: call `compositor.readback()`, send `RgbaFrame` via `composited_frame_tx` to GStreamer encode thread
- On scene switch: diff old vs new source lists. Only `RemoveCaptureSource` for sources not in the new scene, only `AddCaptureSource` for sources not in the old scene. Shared sources (e.g., a webcam used in both scenes) keep running with no interruption.

## Data Flow

1. **Scene activated** → diff against previous scene's sources, send `AddCaptureSource` for new sources only
2. **GStreamer thread** creates capture pipeline per source, pulls frames via non-blocking `try_pull_sample(0ms)` on each appsink sequentially in the run loop, writes latest frame per source to shared `latest_frames` map
3. **Render thread** locks `latest_frames`, drains new frames, uploads to per-source wgpu textures via `compositor.upload_frame()`
4. **Each render frame** → resolve active scene's source IDs to `&Source` refs, pass to `compositor.compose()` which draws all visible sources onto canvas texture in vec order
5. **Preview** samples canvas texture for display (letterboxed)
6. **If streaming/recording** → `compositor.readback()` does `copy_texture_to_buffer` + `submit` + `poll(Wait)` + `map` → sends `RgbaFrame` via `composited_frame_tx` to GStreamer thread, which pushes to encode appsrc(s)
7. **Scene deactivated/switched** → diff source lists, send `RemoveCaptureSource` only for sources no longer needed

## Edge Cases

- **Source added mid-stream**: `AddCaptureSource` starts a new pipeline. Compositor renders remaining sources normally until first frame arrives from new source (source without a `SourceLayer` is skipped in compose).
- **Source removed mid-stream**: `RemoveCaptureSource` stops pipeline. Compositor drops the source layer. Next compose pass skips it.
- **Frame rate mismatch**: Sources may produce frames at different rates. Compositor always uses the latest frame per source — no sync enforcement. This matches OBS behavior.
- **Source texture resize**: If a capture source changes resolution (e.g., display resolution change), `upload_frame` detects size mismatch and recreates the texture.
- **No sources in scene**: Compositor clears canvas to black.
- **All sources invisible**: Same as no sources — black canvas.
- **Orphaned SourceId**: If a scene references a SourceId with no matching Source in the collection, it is silently skipped during source resolution.
- **Scene switch with shared sources**: Diff-based switching keeps shared capture pipelines running. Only sources unique to the old scene are removed; only sources unique to the new scene are added.
- **latest_frames contention**: Lock is held briefly (write one frame / drain all frames). GStreamer thread writes one frame at a time; render thread drains once per frame. Contention is minimal.

## Future Extensions

- **Virtual canvas with output scaling** — design at 4K, stream at 1080p. Add a scaling pass between canvas texture and readback.
- **Source cropping** — add crop rect to Transform, adjust UV coordinates in shader.
- **Blend modes** — multiply, screen, overlay via shader variants.
- **Transitions** — fade/slide between scenes by compositing both scenes with animated opacity.
- **Interactive transform handles** — click-drag sources in the preview panel to reposition/resize.
- **Image/Browser sources** — load static images or render web content to textures, feed into compositor as non-capture sources.
