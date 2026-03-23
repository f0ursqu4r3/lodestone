# Virtual Canvas with Output Scaling Design Spec

Design scenes at high resolution (e.g., 4K) and stream/record at a lower resolution (e.g., 1080p) via a GPU scaling pass.

## Design Decisions

- **GPU scaling in the compositor.** A blit pass scales the canvas texture down to the output texture after compositing. Fast, no CPU overhead, keeps the readback buffer small.
- **Preview shows full resolution.** The preview panel always samples the canvas texture so the user sees the design-time resolution.
- **Settings-driven.** Base and output resolutions come from `state.settings.video.base_resolution` and `state.settings.video.output_resolution`, which already exist in the settings window.

## Architecture

### Two Textures

The compositor manages two textures:

| Texture | Resolution | Purpose |
|---------|-----------|---------|
| `canvas_texture` | Base resolution (e.g., 3840x2160) | Sources composited here. Preview samples this. |
| `output_texture` | Output resolution (e.g., 1920x1080) | Scaled from canvas. Readback reads this for encode. |

Both are `Rgba8UnormSrgb` with `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC` usage.

### Render Pipeline

```
Sources → compose() → canvas_texture (base res)
                          ↓
                    scale_to_output()
                          ↓
                    output_texture (output res)
                          ↓
                    readback() → CPU buffer → encode pipelines
```

The preview panel continues to sample `canvas_texture` directly — it is unaffected by output scaling.

## Scaling Pass

### scale_to_output()

A new method on `Compositor` that renders a fullscreen quad sampling `canvas_texture` and writing to `output_texture`.

```rust
pub fn scale_to_output(&self, encoder: &mut CommandEncoder)
```

**Implementation:** Reuse the existing `canvas_preview_shader` (fullscreen quad that samples a texture). Create a dedicated bind group that binds `canvas_texture_view` + sampler, and a render pass targeting `output_texture_view`.

**Filtering:** Bilinear (the sampler already uses `FilterMode::Linear` for the preview shader). This gives smooth downscaling. For extreme downscale ratios (4K → 720p), bilinear is acceptable — mipmap-based scaling would be better but is unnecessary complexity for now.

### When to Scale

`scale_to_output()` is called after `compose()` and before `readback()`, only when encode pipelines are active (streaming or recording). If nothing is encoding, skip the scale pass to save GPU cycles.

## Resolution Parsing

`state.settings.video.base_resolution` and `output_resolution` are stored as strings like `"1920x1080"`. Parse with:

```rust
fn parse_resolution(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s.split('x').collect();
    let w = parts.get(0).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let h = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    (w, h)
}
```

## Texture Lifecycle

### Initialization

`Compositor::new()` reads base and output resolution from settings and creates both textures at startup.

```rust
pub fn new(device: &Device, base_res: (u32, u32), output_res: (u32, u32)) -> Self
```

The caller (main.rs) parses the resolution strings and passes them in.

### Resolution Changes

When the user changes base or output resolution in settings:
1. Detect the change (compare parsed resolution to current texture dimensions)
2. Recreate the affected texture(s)
3. Recreate bind groups that reference the changed texture
4. Recreate the readback buffer if output resolution changed

This check happens once per frame in the render loop (cheap — just compare two `u32` pairs).

### Same Resolution Optimization

If base and output resolutions are identical, skip `scale_to_output()` entirely and readback directly from `canvas_texture`. No output texture needed.

## Readback Changes

Currently `readback()` copies from `canvas_texture`. Change to copy from `output_texture`:

- `readback_buffer` sized to `output_width * output_height * 4` bytes
- `encoder.copy_texture_to_buffer()` targets `output_texture`
- The `RgbaFrame` sent to encode pipelines has output resolution dimensions

## Compositor API Changes

```rust
// Current
pub fn new(device: &Device, queue: &Queue) -> Self  // Fixed 1920x1080

// New
pub fn new(device: &Device, queue: &Queue, base_res: (u32, u32), output_res: (u32, u32)) -> Self

// New method
pub fn scale_to_output(&self, encoder: &mut CommandEncoder)

// New method — call when settings change
pub fn resize(&mut self, device: &Device, base_res: (u32, u32), output_res: (u32, u32))
```

## Main Loop Changes

In `src/main.rs`, the render loop currently calls:
1. `compositor.compose(...)`
2. `compositor.readback(...)`

Change to:
1. `compositor.compose(...)`
2. `compositor.scale_to_output(...)` (only if encoding active)
3. `compositor.readback(...)` (reads from output texture)

Also: parse resolution from settings and pass to `Compositor::new()`. On settings change, call `compositor.resize()`.

## File Structure

```
src/renderer/compositor.rs  # MODIFY — output_texture, scale_to_output(), resize(), readback changes
src/main.rs                 # MODIFY — parse resolutions, pass to compositor, call scale_to_output
```
