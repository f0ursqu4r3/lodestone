# Animated GIF Support Design

## Overview

Add animated GIF support to image sources. When an image source points to a `.gif` file with multiple frames, decode all frames upfront and animate them in the render loop at the correct frame rate. Users can control loop behavior via source properties.

## Data Model

### ImageData Return Type

`load_image_source()` currently returns `RgbaFrame`. Change it to return an enum:

```rust
pub enum ImageData {
    Static(RgbaFrame),
    Animated(GifAnimation),
}
```

### GifAnimation Struct

```rust
pub struct GifAnimation {
    pub frames: Vec<RgbaFrame>,
    pub delays: Vec<Duration>,
    pub embedded_loop_count: LoopMode,
}
```

All frames decoded upfront into RGBA8. `delays[i]` is the display duration for `frames[i]`.

### LoopMode

```rust
pub enum LoopMode {
    Infinite,
    Once,
    Count(u32),
}
```

### SourceProperties::Image Extension

Add a `loop_mode` field to the Image variant:

```rust
Image {
    path: String,
    /// Override for GIF loop behavior. None = use GIF's embedded loop count.
    loop_mode: Option<LoopMode>,
}
```

Backwards compatible via `#[serde(default)]` — existing image sources deserialize with `loop_mode: None`.

### Runtime Animation State

Stored on the main thread in a `HashMap<SourceId, AnimationState>`:

```rust
struct AnimationState {
    animation: GifAnimation,
    current_frame: usize,
    frame_started_at: Instant,
    loop_mode: LoopMode,
    loops_completed: u32,
    finished: bool,
}
```

This map lives on `EventHandler` alongside the existing GPU and channel state. Entries are added when an animated GIF is loaded and removed when the source is removed.

## Load Path

1. `load_image_source(path)` checks if the file is a GIF with multiple frames.
2. If animated GIF: decode all frames using the `image` crate's GIF decoder, extract frame delays, return `ImageData::Animated(GifAnimation)`.
3. If static image (PNG, JPEG, single-frame GIF, etc.): return `ImageData::Static(RgbaFrame)` as today.
4. The caller (`load_and_send_image` in properties panel, or initial scene load) handles both variants:
   - Static: sends `GstCommand::LoadImageFrame` as today.
   - Animated: stores `AnimationState` in the main thread's animation map, sends the first frame immediately.

## Render Loop Integration

In `about_to_wait()`, before the composition section, iterate the animation map:

1. For each `(source_id, anim_state)`:
   - If `finished`, skip.
   - If `frame_started_at.elapsed() >= delays[current_frame]`:
     - Advance `current_frame`.
     - If past the last frame: increment `loops_completed`, check loop mode.
       - `Infinite`: wrap to frame 0.
       - `Once`: set `finished = true`, stay on last frame.
       - `Count(n)`: if `loops_completed >= n`, set `finished = true`; else wrap to frame 0.
     - Upload `frames[current_frame]` via `compositor.upload_frame()`.
     - Reset `frame_started_at = Instant::now()`.
2. If any animation is active (not finished), call `window.request_redraw()` to drive continuous updates.

This runs on the main thread which already handles frame uploads. No cross-thread complexity.

## Properties UI

In the image source properties section of `src/ui/properties_panel.rs`:

- Detect whether the current source is an animated GIF (check if it has an entry in the animation map, or check if the file extension is `.gif` and has multiple frames).
- If animated: show a "Loop" dropdown with options: Default (use GIF metadata), Infinite, Once, Count (with numeric input).
- Changes update `SourceProperties::Image.loop_mode` and the runtime `AnimationState.loop_mode`.
- Static images don't show the loop control.

## Source Lifecycle

- **Source added/loaded:** If the image path is an animated GIF, decode and store in animation map. Send first frame.
- **Source removed:** Remove from animation map.
- **Path changed:** Remove old animation state, load new file, store new state if animated.
- **Source hidden:** Animation continues advancing (so it's in the right position when shown). Frame uploads can be skipped for hidden sources as an optimization, but the timer still advances.

## What Doesn't Change

- GStreamer thread — images bypass it entirely, that continues.
- Compositor — receives `RgbaFrame` via `upload_frame()` as today. Doesn't know or care about animation.
- Transition system, scene model (beyond the one field on SourceProperties::Image).
- Secondary canvas — GIF frames upload to both primary and secondary canvas via the existing dual-upload path.

## Testing

- Unit test: `load_image_source` on a static PNG returns `ImageData::Static`.
- Unit test: `load_image_source` on an animated GIF returns `ImageData::Animated` with correct frame count and delays.
- Unit test: `LoopMode` serialization roundtrip.
- Unit test: `AnimationState` frame advancement logic (wrap, finish, count).
- Visual: animated GIF plays in Preview panel at correct speed, loops correctly.
