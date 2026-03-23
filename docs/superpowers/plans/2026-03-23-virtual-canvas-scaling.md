# Virtual Canvas with Output Scaling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support designing at one resolution (e.g., 4K) and streaming/recording at another (e.g., 1080p) via a GPU scaling pass in the compositor.

**Architecture:** The compositor gains a second `output_texture` at the configured output resolution. After compositing sources onto the canvas texture, a `scale_to_output()` method renders a fullscreen quad that samples the canvas and writes to the output texture with bilinear filtering. Readback copies from the output texture instead of the canvas.

**Tech Stack:** Rust, wgpu (textures, render passes, samplers)

**Spec:** `docs/superpowers/specs/2026-03-23-virtual-canvas-scaling-design.md`

---

## File Structure

```
src/renderer/compositor.rs  # MODIFY — output texture, scale pass, resize, readback changes
src/main.rs                 # MODIFY — parse resolutions, pass to compositor, call scale_to_output
```

---

### Task 1: Make Canvas Resolution Configurable

**Files:**
- Modify: `src/renderer/compositor.rs`
- Modify: `src/main.rs`

Currently the compositor's canvas is hardcoded to 1920x1080. Make it configurable.

- [ ] **Step 1: Change `Compositor::new()` signature**

Add `base_res: (u32, u32)` parameter. Replace all hardcoded `1920`/`1080` canvas dimensions with the parameter values. Store them as fields:

```rust
pub canvas_width: u32,
pub canvas_height: u32,
```

- [ ] **Step 2: Add resolution parsing helper**

In `src/renderer/compositor.rs` (or a shared location):

```rust
/// Parse a "WIDTHxHEIGHT" string into (width, height). Defaults to 1920x1080.
pub fn parse_resolution(s: &str) -> (u32, u32) {
    let parts: Vec<&str> = s.split('x').collect();
    let w = parts.first().and_then(|s| s.parse().ok()).unwrap_or(1920);
    let h = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);
    (w, h)
}
```

- [ ] **Step 3: Update main.rs to pass resolution**

Find where `Compositor::new()` is called and pass the parsed base resolution from settings:

```rust
let base_res = parse_resolution(&state.settings.video.base_resolution);
let compositor = Compositor::new(&device, &queue, base_res);
```

- [ ] **Step 4: Update tests**

Any tests that create a `Compositor` need the new parameter. Update them to pass `(1920, 1080)` or appropriate test dimensions.

- [ ] **Step 5: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 6: Commit**

```bash
git add src/renderer/compositor.rs src/main.rs
git commit -m "refactor(compositor): make canvas resolution configurable"
```

---

### Task 2: Add Output Texture and Scale Pass

**Files:**
- Modify: `src/renderer/compositor.rs`

- [ ] **Step 1: Add output texture fields**

Add to Compositor struct:

```rust
pub output_width: u32,
pub output_height: u32,
output_texture: wgpu::Texture,
output_texture_view: wgpu::TextureView,
output_bind_group: wgpu::BindGroup,  // canvas_texture_view + sampler for the scale pass
```

Update `new()` to accept `output_res: (u32, u32)` and create the output texture with the same format/usage as canvas but at output dimensions.

Create a bind group that binds `canvas_texture_view` + the existing sampler for the scale shader to read from.

- [ ] **Step 2: Implement scale_to_output()**

```rust
/// Scale the canvas texture down to the output texture via a fullscreen quad.
/// Only call when encode pipelines are active.
pub fn scale_to_output(&self, encoder: &mut wgpu::CommandEncoder) {
    if self.canvas_width == self.output_width && self.canvas_height == self.output_height {
        return; // Same resolution — no-op
    }

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("scale_to_output"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &self.output_texture_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        ..Default::default()
    });

    // Use the canvas preview pipeline (fullscreen quad sampling a texture)
    pass.set_pipeline(&self.canvas_preview_pipeline);
    pass.set_bind_group(0, &self.output_bind_group, &[]);
    pass.draw(0..4, 0..1);
}
```

This reuses the existing `canvas_preview_pipeline` which already does a fullscreen quad texture sample. The only difference is: instead of targeting the screen surface, it targets `output_texture_view`. And the bind group binds `canvas_texture_view` instead of whatever the preview normally binds.

The implementer should check that the `canvas_preview_pipeline` is compatible with a render target of the output texture format. If the pipeline was created with the surface format (which may differ from `Rgba8UnormSrgb`), a separate pipeline may be needed.

- [ ] **Step 3: Build and verify**

Run: `cargo build`

- [ ] **Step 4: Commit**

```bash
git add src/renderer/compositor.rs
git commit -m "feat(compositor): add output texture and scale_to_output pass"
```

---

### Task 3: Update Readback to Use Output Texture

**Files:**
- Modify: `src/renderer/compositor.rs`

- [ ] **Step 1: Change readback to copy from output texture**

In the `readback()` method (or wherever `copy_texture_to_buffer` is called):

- Change the source from `canvas_texture` to `output_texture`
- Size the readback buffer to `output_width * output_height * 4`
- The `RgbaFrame` returned should have `output_width` and `output_height` as dimensions

When resolutions are equal (no scaling), read from `canvas_texture` directly (skip the output texture entirely).

- [ ] **Step 2: Update readback buffer creation**

The readback buffer is created in `new()`. Size it to output dimensions:

```rust
let readback_size = (output_width * output_height * 4) as u64;
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 4: Commit**

```bash
git add src/renderer/compositor.rs
git commit -m "feat(compositor): readback from output texture at output resolution"
```

---

### Task 4: Wire Scale Pass into Render Loop

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Pass output resolution to compositor**

Update the `Compositor::new()` call to include output resolution:

```rust
let base_res = parse_resolution(&state.settings.video.base_resolution);
let output_res = parse_resolution(&state.settings.video.output_resolution);
let compositor = Compositor::new(&device, &queue, base_res, output_res);
```

- [ ] **Step 2: Call scale_to_output in the render loop**

Find where `compositor.compose()` and `compositor.readback()` are called. Insert the scale pass between them:

```rust
compositor.compose(&queue, &mut encoder, &visible_sources);
// Scale to output resolution before readback (only when encoding)
if is_streaming || is_recording {
    compositor.scale_to_output(&mut encoder);
}
compositor.readback(&mut encoder);
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire scale_to_output into render loop"
```

---

### Task 5: Resolution Change Handling

**Files:**
- Modify: `src/renderer/compositor.rs` (add `resize()` method)
- Modify: `src/main.rs` (detect resolution changes, call resize)

- [ ] **Step 1: Add resize() method**

```rust
/// Recreate textures when resolution settings change.
pub fn resize(&mut self, device: &wgpu::Device, base_res: (u32, u32), output_res: (u32, u32)) {
    let (bw, bh) = base_res;
    let (ow, oh) = output_res;

    if bw != self.canvas_width || bh != self.canvas_height {
        // Recreate canvas texture, canvas texture view, canvas bind group
        // ... (same creation code as new(), just for the canvas)
        self.canvas_width = bw;
        self.canvas_height = bh;
    }

    if ow != self.output_width || oh != self.output_height {
        // Recreate output texture, output texture view, output bind group, readback buffer
        // ... (same creation code as new(), just for the output)
        self.output_width = ow;
        self.output_height = oh;
    }
}
```

The implementer should extract the texture creation code into helpers to avoid duplication between `new()` and `resize()`.

- [ ] **Step 2: Detect resolution changes in main loop**

In the render loop, check if resolution settings have changed:

```rust
let new_base = parse_resolution(&state.settings.video.base_resolution);
let new_output = parse_resolution(&state.settings.video.output_resolution);
if new_base != (gpu.compositor.canvas_width, gpu.compositor.canvas_height)
    || new_output != (gpu.compositor.output_width, gpu.compositor.output_height)
{
    gpu.compositor.resize(&gpu.device, new_base, new_output);
}
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 4: Commit**

```bash
git add src/renderer/compositor.rs src/main.rs
git commit -m "feat(compositor): handle runtime resolution changes"
```

---

### Task 6: Final Integration

- [ ] **Step 1: Build, test, clippy, fmt**

Run: `cargo build && cargo test && cargo clippy && cargo fmt --check`
Fix any issues.

- [ ] **Step 2: Commit fixes**

```bash
git add -A
git commit -m "chore: final integration for virtual canvas scaling"
```
