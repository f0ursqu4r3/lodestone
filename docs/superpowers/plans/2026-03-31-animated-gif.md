# Animated GIF Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add animated GIF support to image sources — decode all frames upfront, animate in the render loop, with user-configurable loop behavior.

**Architecture:** Extend `image_source.rs` to detect and decode animated GIFs into a `GifAnimation` struct (all frames + delays). Store runtime animation state in a `HashMap<SourceId, AnimationState>` on `AppManager`. The render loop advances frame timers and uploads new frames via the existing `compositor.upload_frame()` path. A `loop_mode` field on `SourceProperties::Image` lets users control looping.

**Tech Stack:** Rust, `image` crate (already a dependency, supports GIF decoding), wgpu.

---

## File Structure

| Action | File | Responsibility |
|--------|------|---------------|
| Modify | `src/image_source.rs` | Detect GIFs, decode all frames, return `ImageData` enum |
| Modify | `src/scene.rs:185-187` | Add `loop_mode` field to `SourceProperties::Image` |
| Modify | `src/main.rs:193-209` | Add `gif_animations: HashMap` to `AppManager`, add animation tick |
| Modify | `src/ui/properties_panel.rs:1737-1767` | Handle `ImageData::Animated`, add loop mode UI |
| Modify | `src/ui/library_panel.rs` | Handle animated GIF loads from library |

---

## Task 1: Types and GIF Decoding

**Files:**
- Modify: `src/image_source.rs`
- Modify: `src/scene.rs:185-187`

- [ ] **Step 1: Add `LoopMode` to `src/scene.rs`**

Add the enum near the other source-related types, and add the `loop_mode` field to `SourceProperties::Image`:

```rust
/// How an animated image (GIF) should loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LoopMode {
    #[default]
    Infinite,
    Once,
    Count(u32),
}
```

Update `SourceProperties::Image`:
```rust
Image {
    path: String,
    /// Loop behavior override for animated GIFs. None = use GIF's embedded loop count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    loop_mode: Option<LoopMode>,
},
```

- [ ] **Step 2: Add `ImageData` and `GifAnimation` to `src/image_source.rs`**

```rust
use std::time::Duration;

/// Result of loading an image file — either a single frame or an animation.
pub enum ImageData {
    Static(RgbaFrame),
    Animated(GifAnimation),
}

/// All frames of an animated GIF, decoded upfront.
pub struct GifAnimation {
    pub frames: Vec<RgbaFrame>,
    pub delays: Vec<Duration>,
    /// Loop count from the GIF metadata.
    pub embedded_loop_count: crate::scene::LoopMode,
}
```

- [ ] **Step 3: Implement animated GIF decoding**

Replace `load_image_source` with a function that returns `ImageData`:

```rust
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, ImageReader};
use std::fs::File;
use std::io::BufReader;

/// Load an image file. Returns `Animated` for multi-frame GIFs, `Static` for everything else.
pub fn load_image_source(path: &str) -> anyhow::Result<ImageData> {
    // Check if it's a GIF by trying to decode as animated GIF first.
    if path.to_lowercase().ends_with(".gif") {
        let file = File::open(path)
            .with_context(|| format!("Failed to open image: {path}"))?;
        let decoder = GifDecoder::new(BufReader::new(file))
            .with_context(|| format!("Failed to decode GIF: {path}"))?;

        let frames_iter = decoder.into_frames();
        let mut frames = Vec::new();
        let mut delays = Vec::new();

        for frame_result in frames_iter {
            let frame = frame_result
                .with_context(|| format!("Failed to decode GIF frame: {path}"))?;
            let (numer, denom) = frame.delay().numer_denom_ms();
            let delay_ms = (numer as f64 / denom as f64) as u64;
            // GIF spec: 0ms delay means "as fast as possible", commonly treated as 100ms.
            let delay_ms = if delay_ms < 20 { 100 } else { delay_ms };
            delays.push(Duration::from_millis(delay_ms));

            let rgba_image = frame.into_buffer();
            let width = rgba_image.width();
            let height = rgba_image.height();
            let data = rgba_image.into_raw();
            frames.push(RgbaFrame { data, width, height });
        }

        if frames.len() > 1 {
            return Ok(ImageData::Animated(GifAnimation {
                frames,
                delays,
                embedded_loop_count: crate::scene::LoopMode::Infinite, // image crate doesn't expose loop count easily
            }));
        }

        // Single-frame GIF — treat as static.
        if let Some(frame) = frames.into_iter().next() {
            return Ok(ImageData::Static(frame));
        }

        anyhow::bail!("GIF has no frames: {path}");
    }

    // Non-GIF: load as static image.
    let img = image::open(path)
        .with_context(|| format!("Failed to open image: {path}"))?
        .into_rgba8();
    let width = img.width();
    let height = img.height();
    let data = img.into_raw();
    Ok(ImageData::Static(RgbaFrame { data, width, height }))
}
```

- [ ] **Step 4: Update existing tests and add new ones**

Update the existing test and add GIF-specific tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_image_source("/nonexistent/path.png");
        assert!(result.is_err());
    }

    #[test]
    fn load_nonexistent_gif_returns_error() {
        let result = load_image_source("/nonexistent/path.gif");
        assert!(result.is_err());
    }

    #[test]
    fn loop_mode_default_is_infinite() {
        assert_eq!(crate::scene::LoopMode::default(), crate::scene::LoopMode::Infinite);
    }
}
```

- [ ] **Step 5: Run `cargo build && cargo test`**

Run: `cargo build && cargo test 2>&1 | tail -10`
Expected: Compiles and all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/image_source.rs src/scene.rs
git commit -m "feat: add animated GIF decoding and LoopMode types"
```

---

## Task 2: Animation State and Render Loop Tick

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add `AnimationState` struct and `gif_animations` map to AppManager**

Near the top of `src/main.rs` (after the imports), add:

```rust
/// Runtime state for an active GIF animation.
struct AnimationState {
    frames: Vec<crate::gstreamer::RgbaFrame>,
    delays: Vec<std::time::Duration>,
    current_frame: usize,
    frame_started_at: std::time::Instant,
    loop_mode: crate::scene::LoopMode,
    loops_completed: u32,
    finished: bool,
}
```

Add to `AppManager` struct (after `settings_window_id`):

```rust
/// Active GIF animations, keyed by source ID.
gif_animations: std::collections::HashMap<crate::scene::SourceId, AnimationState>,
```

Initialize in `AppManager::new()`:

```rust
gif_animations: std::collections::HashMap::new(),
```

- [ ] **Step 2: Add animation tick in `about_to_wait()`**

In the `about_to_wait()` method, BEFORE the composition section (before `if let Some(ref mut gpu) = self.gpu`), add the animation tick:

```rust
// Advance GIF animations and upload new frames.
{
    let mut any_active = false;
    let gpu_ref = self.gpu.as_mut();
    for (source_id, anim) in self.gif_animations.iter_mut() {
        if anim.finished {
            continue;
        }
        any_active = true;
        let elapsed = anim.frame_started_at.elapsed();
        if elapsed >= anim.delays[anim.current_frame] {
            // Advance to next frame.
            anim.current_frame += 1;
            if anim.current_frame >= anim.frames.len() {
                anim.loops_completed += 1;
                match anim.loop_mode {
                    crate::scene::LoopMode::Infinite => {
                        anim.current_frame = 0;
                    }
                    crate::scene::LoopMode::Once => {
                        anim.current_frame = anim.frames.len() - 1;
                        anim.finished = true;
                        continue;
                    }
                    crate::scene::LoopMode::Count(n) => {
                        if anim.loops_completed >= n {
                            anim.current_frame = anim.frames.len() - 1;
                            anim.finished = true;
                            continue;
                        }
                        anim.current_frame = 0;
                    }
                }
            }
            anim.frame_started_at = std::time::Instant::now();
            // Upload the new frame.
            if let Some(ref mut gpu) = gpu_ref {
                let frame = &anim.frames[anim.current_frame];
                gpu.compositor.upload_frame(&gpu.device, &gpu.queue, *source_id, frame);
                if let Some(ref mut secondary) = gpu.secondary_canvas {
                    secondary.upload_frame(
                        &gpu.device,
                        &gpu.queue,
                        *source_id,
                        frame,
                        gpu.compositor.texture_bind_group_layout(),
                        gpu.compositor.uniform_bind_group_layout(),
                        gpu.compositor.compositor_sampler(),
                    );
                }
            }
        }
    }
    // Drive redraws while any animation is active.
    if any_active {
        if let Some(main_id) = self.main_window_id
            && let Some(win) = self.windows.get(&main_id)
        {
            win.window.request_redraw();
        }
    }
}
```

Note: the borrow checker may require restructuring the `gpu_ref` access. The implementer should adjust as needed — the important thing is: iterate animations, advance frames, upload to compositor. If borrowing `self.gpu` and `self.gif_animations` simultaneously is an issue, collect the frames-to-upload into a `Vec<(SourceId, &RgbaFrame)>` first, then do the uploads.

- [ ] **Step 3: Run `cargo build`**

Run: `cargo build 2>&1 | tail -10`
Expected: Compiles. The animation map is empty so nothing fires yet.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add GIF animation state and render loop tick"
```

---

## Task 3: Wire Up Image Loading for Animated GIFs

**Files:**
- Modify: `src/ui/properties_panel.rs` (load_and_send_image function)
- Modify: `src/main.rs` (need a way for UI to register animations)

- [ ] **Step 1: Add a GstCommand variant for animated GIF registration**

The UI loads images and currently sends `GstCommand::LoadImageFrame`. For animated GIFs, we need to get the animation data to the main thread's `gif_animations` map. The simplest approach: add a new command or use a separate channel.

Actually, the simplest approach: store the animation data on `AppState` temporarily, and have the main thread pick it up. Add to `AppState`:

```rust
/// Pending GIF animations to register, set by UI, consumed by render loop.
pub pending_gif_animations: Vec<(SourceId, crate::image_source::GifAnimation, crate::scene::LoopMode)>,
```

Default: `pending_gif_animations: Vec::new(),`

- [ ] **Step 2: Update `load_and_send_image` in properties_panel.rs**

Change the function to handle both `ImageData::Static` and `ImageData::Animated`:

```rust
fn load_and_send_image(
    state: &mut AppState,
    source_idx: usize,
    source_id: crate::scene::SourceId,
    cmd_tx: &Option<tokio::sync::mpsc::Sender<GstCommand>>,
    path: String,
) {
    match crate::image_source::load_image_source(&path) {
        Ok(crate::image_source::ImageData::Static(frame)) => {
            let source = &mut state.library[source_idx];
            if let SourceProperties::Image { path: ref mut p, .. } = source.properties {
                *p = path;
            }
            let native = (frame.width as f32, frame.height as f32);
            source.transform.width = native.0;
            source.transform.height = native.1;
            source.native_size = native;
            if let Some(tx) = cmd_tx {
                let _ = tx.try_send(GstCommand::LoadImageFrame { source_id, frame });
            }
        }
        Ok(crate::image_source::ImageData::Animated(animation)) => {
            let source = &mut state.library[source_idx];
            if let SourceProperties::Image { path: ref mut p, .. } = source.properties {
                *p = path;
            }
            // Set transform from first frame dimensions.
            if let Some(first) = animation.frames.first() {
                let native = (first.width as f32, first.height as f32);
                source.transform.width = native.0;
                source.transform.height = native.1;
                source.native_size = native;
                // Send first frame immediately so it shows up right away.
                if let Some(tx) = cmd_tx {
                    let _ = tx.try_send(GstCommand::LoadImageFrame {
                        source_id,
                        frame: first.clone(),
                    });
                }
            }
            // Determine loop mode.
            let loop_mode = if let SourceProperties::Image { loop_mode: Some(lm), .. } = &source.properties {
                *lm
            } else {
                animation.embedded_loop_count
            };
            // Queue animation for the render loop to pick up.
            state.pending_gif_animations.push((source_id, animation, loop_mode));
        }
        Err(e) => {
            state.active_errors.push(GstError::CaptureFailure {
                message: format!("Failed to load image: {e}"),
            });
        }
    }
}
```

- [ ] **Step 3: Consume pending animations in the render loop**

In `about_to_wait()`, before the animation tick block, drain pending animations from AppState:

```rust
{
    let mut app_state = self.state.lock().expect("lock AppState");
    for (source_id, animation, loop_mode) in app_state.pending_gif_animations.drain(..) {
        self.gif_animations.insert(source_id, AnimationState {
            frames: animation.frames,
            delays: animation.delays,
            current_frame: 0,
            frame_started_at: std::time::Instant::now(),
            loop_mode,
            loops_completed: 0,
            finished: false,
        });
    }
}
```

- [ ] **Step 4: Clean up animations when sources are removed**

Search for where `RemoveCaptureSource` is handled or where sources are deleted. When a source is removed, also remove its animation state:

In the scene switching/source removal paths, add:
```rust
self.gif_animations.remove(&source_id);
```

The simplest place: after draining `pending_gif_animations`, also check if any animations reference sources that no longer exist in the library, and remove them.

- [ ] **Step 5: Fix all other `load_image_source` call sites**

Search for all places that call `load_image_source` or send `LoadImageFrame` directly (not through `load_and_send_image`). These are in:
- `src/ui/library_panel.rs` — multiple sites
- `src/ui/properties_panel.rs` — other sites besides `load_and_send_image`

For each site, the pattern should be:
- If it currently calls `load_image_source` and gets an `RgbaFrame`, update to handle `ImageData` enum
- For `ImageData::Static`: same as before
- For `ImageData::Animated`: send first frame + push to `pending_gif_animations`

The implementer should search for all call sites and update them. The pattern is always the same.

- [ ] **Step 6: Run `cargo build && cargo test`**

Run: `cargo build && cargo test 2>&1 | tail -10`
Expected: Compiles and all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/state.rs src/ui/properties_panel.rs src/ui/library_panel.rs
git commit -m "feat: wire up animated GIF loading and render loop consumption"
```

---

## Task 4: Loop Mode UI in Properties Panel

**Files:**
- Modify: `src/ui/properties_panel.rs`

- [ ] **Step 1: Add loop mode control to image source properties**

In the properties panel, find where `SourceProperties::Image` is rendered (look for the path field / file picker). After the image path controls, add a loop mode dropdown that only appears for animated GIFs:

```rust
// Loop mode control — only for animated GIFs
let is_animated = state.library.get(lib_idx)
    .map(|s| matches!(&s.properties, SourceProperties::Image { path, .. } if path.to_lowercase().ends_with(".gif")))
    .unwrap_or(false);

if is_animated {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Loop:");
        let current_mode = if let SourceProperties::Image { loop_mode, .. } = &state.library[lib_idx].properties {
            loop_mode.unwrap_or(crate::scene::LoopMode::Infinite)
        } else {
            crate::scene::LoopMode::Infinite
        };

        let mode_label = match current_mode {
            crate::scene::LoopMode::Infinite => "Infinite",
            crate::scene::LoopMode::Once => "Once",
            crate::scene::LoopMode::Count(_) => "Count",
        };

        egui::ComboBox::from_id_salt("gif_loop_mode")
            .selected_text(mode_label)
            .show_ui(ui, |ui| {
                let source = &mut state.library[lib_idx];
                if let SourceProperties::Image { ref mut loop_mode, .. } = source.properties {
                    if ui.selectable_label(matches!(current_mode, crate::scene::LoopMode::Infinite), "Infinite").clicked() {
                        *loop_mode = Some(crate::scene::LoopMode::Infinite);
                        state.mark_dirty();
                    }
                    if ui.selectable_label(matches!(current_mode, crate::scene::LoopMode::Once), "Once").clicked() {
                        *loop_mode = Some(crate::scene::LoopMode::Once);
                        state.mark_dirty();
                    }
                    if ui.selectable_label(matches!(current_mode, crate::scene::LoopMode::Count(_)), "Count").clicked() {
                        *loop_mode = Some(crate::scene::LoopMode::Count(3));
                        state.mark_dirty();
                    }
                }
            });

        // Count input when Count mode is selected
        if let crate::scene::LoopMode::Count(ref mut n) = current_mode.clone() {
            // Show inline count input
            if let SourceProperties::Image { ref mut loop_mode, .. } = state.library[lib_idx].properties {
                if let Some(crate::scene::LoopMode::Count(ref mut count)) = loop_mode {
                    let mut count_str = count.to_string();
                    let resp = ui.add(egui::TextEdit::singleline(&mut count_str).desired_width(30.0));
                    if resp.changed() {
                        if let Ok(val) = count_str.parse::<u32>() {
                            *count = val.max(1);
                            state.mark_dirty();
                        }
                    }
                }
            }
        }
    });
}
```

- [ ] **Step 2: Update runtime animation when loop mode changes**

When the user changes `loop_mode` in the UI, also update the running `AnimationState`. Add to `pending_gif_animations` or use a simpler mechanism: store loop mode updates on AppState:

```rust
/// Pending loop mode updates for GIF animations.
pub pending_loop_mode_updates: Vec<(SourceId, LoopMode)>,
```

Consume in the render loop alongside `pending_gif_animations`:
```rust
for (source_id, new_mode) in app_state.pending_loop_mode_updates.drain(..) {
    if let Some(anim) = self.gif_animations.get_mut(&source_id) {
        anim.loop_mode = new_mode;
        anim.finished = false; // Reset in case it was finished with old mode
        anim.loops_completed = 0;
    }
}
```

- [ ] **Step 3: Run `cargo build && cargo test`**

Run: `cargo build && cargo test 2>&1 | tail -10`
Expected: Compiles and all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/ui/properties_panel.rs src/state.rs src/main.rs
git commit -m "feat: add loop mode UI for animated GIF sources"
```

---

## Task 5: Tests and Final Verification

**Files:**
- Modify: `src/image_source.rs` (add tests)

- [ ] **Step 1: Add animation state unit tests**

Add tests to `src/image_source.rs`:

```rust
#[test]
fn static_image_data_variant() {
    let frame = RgbaFrame { data: vec![0; 4], width: 1, height: 1 };
    let data = ImageData::Static(frame);
    assert!(matches!(data, ImageData::Static(_)));
}

#[test]
fn animated_image_data_variant() {
    let frames = vec![
        RgbaFrame { data: vec![0; 4], width: 1, height: 1 },
        RgbaFrame { data: vec![255; 4], width: 1, height: 1 },
    ];
    let delays = vec![Duration::from_millis(100), Duration::from_millis(200)];
    let data = ImageData::Animated(GifAnimation {
        frames,
        delays,
        embedded_loop_count: crate::scene::LoopMode::Infinite,
    });
    if let ImageData::Animated(ref anim) = data {
        assert_eq!(anim.frames.len(), 2);
        assert_eq!(anim.delays.len(), 2);
        assert_eq!(anim.delays[0], Duration::from_millis(100));
    } else {
        panic!("Expected Animated variant");
    }
}
```

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -30`
Fix any warnings in changed files.

- [ ] **Step 3: Run fmt**

Run: `cargo fmt --check` — fix if needed.

- [ ] **Step 4: Run full test suite**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/image_source.rs
git commit -m "test: add animated GIF unit tests, fix clippy/fmt"
```
