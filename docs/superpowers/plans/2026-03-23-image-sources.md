# Image Sources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add static image files (PNG, JPEG, etc.) as compositor sources, loaded from disk and pushed as frames through the GStreamer command channel.

**Architecture:** Image files are decoded to RGBA via the `image` crate, then sent to the GStreamer thread as `GstCommand::LoadImageFrame`. The GStreamer thread inserts the frame into the shared frame map. The compositor picks it up like any other source frame. No GStreamer pipeline needed per image — just a single frame push.

**Tech Stack:** Rust, `image` crate (decoding), `rfd` crate (native file dialog), egui

**Spec:** `docs/superpowers/specs/2026-03-23-image-sources-design.md`

---

## File Structure

```
src/image_source.rs           # NEW — load_image_source() function
src/scene.rs                  # MODIFY — SourceProperties::Image variant
src/gstreamer/commands.rs     # MODIFY — GstCommand::LoadImageFrame variant
src/gstreamer/thread.rs       # MODIFY — handle LoadImageFrame
src/ui/sources_panel.rs       # MODIFY — Image in type picker, guard remove
src/ui/properties_panel.rs    # MODIFY — path input, browse, reload
src/main.rs                   # MODIFY — skip Image in capture startup loop
Cargo.toml                    # MODIFY — add image, rfd dependencies
```

---

### Task 1: Add Dependencies and Data Model

**Files:**
- Modify: `Cargo.toml` (add `image`, `rfd`)
- Modify: `src/scene.rs` (add `SourceProperties::Image`)
- Modify: `src/gstreamer/commands.rs` (add `GstCommand::LoadImageFrame`)
- Modify: `src/gstreamer/thread.rs` (handle `LoadImageFrame`)
- Fix exhaustive matches in `src/main.rs`, `src/ui/scenes_panel.rs`

- [ ] **Step 1: Add dependencies**

```bash
cargo add image rfd
```

- [ ] **Step 2: Add SourceProperties::Image**

In `src/scene.rs`, add to the `SourceProperties` enum:

```rust
Image { path: String },
```

- [ ] **Step 3: Add GstCommand::LoadImageFrame**

In `src/gstreamer/commands.rs`, add to the `GstCommand` enum:

```rust
LoadImageFrame { source_id: SourceId, frame: RgbaFrame },
```

Note: `RgbaFrame` is already defined in the gstreamer module.

- [ ] **Step 4: Handle LoadImageFrame in GStreamer thread**

In `src/gstreamer/thread.rs`, in the command match, add:

```rust
GstCommand::LoadImageFrame { source_id, frame } => {
    self.channels.latest_frames.lock().unwrap().insert(source_id, frame);
}
```

- [ ] **Step 5: Fix exhaustive matches**

In `src/main.rs` — the startup loop that maps `SourceProperties` to `CaptureSourceConfig` for initial captures: add an `Image` arm that does nothing (image sources don't have capture pipelines).

In `src/ui/scenes_panel.rs` — same pattern if there's a match on properties for scene switching.

In `src/ui/properties_panel.rs` — add a placeholder arm for Image (will be filled in Task 3).

- [ ] **Step 6: Build and verify**

Run: `cargo build`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/scene.rs src/gstreamer/commands.rs \
  src/gstreamer/thread.rs src/main.rs src/ui/scenes_panel.rs src/ui/properties_panel.rs
git commit -m "feat(model): add Image source variant and LoadImageFrame command"
```

---

### Task 2: Image Loading Function

**Files:**
- Create: `src/image_source.rs`
- Modify: `src/main.rs` (add module declaration)

- [ ] **Step 1: Create `src/image_source.rs`**

```rust
//! Image source loading — decodes image files to RgbaFrame.

use anyhow::Context;
use crate::gstreamer::RgbaFrame;

/// Load an image file and convert to an RgbaFrame.
pub fn load_image_source(path: &str) -> anyhow::Result<RgbaFrame> {
    let img = image::open(path)
        .with_context(|| format!("Failed to open image: {path}"))?
        .into_rgba8();
    let width = img.width();
    let height = img.height();
    let data = img.into_raw();
    Ok(RgbaFrame { data, width, height })
}
```

- [ ] **Step 2: Add module declaration**

In `src/main.rs`, add `mod image_source;` near the other module declarations.

- [ ] **Step 3: Write test**

In `src/image_source.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_image_source("/nonexistent/path.png");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test image_source`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/image_source.rs src/main.rs
git commit -m "feat: add image source loading function"
```

---

### Task 3: Image Source in UI (Sources + Properties)

**Files:**
- Modify: `src/ui/sources_panel.rs` (add Image to type picker, guard remove)
- Modify: `src/ui/properties_panel.rs` (path input, browse, reload)

- [ ] **Step 1: Add Image to source type picker**

In `src/ui/sources_panel.rs`, in the source type popup menu (added in the window/camera feature), add an "Image" option:

```rust
if ui.button("Image").clicked() {
    add_source = Some(SourceType::Image);
    ui.memory_mut(|m| m.close_popup());
}
```

- [ ] **Step 2: Add add_image_source function**

```rust
fn add_image_source(state: &mut AppState, _cmd_tx: &Option<Sender<GstCommand>>, scene_id: SceneId) {
    let source_id = SourceId(state.next_source_id);
    state.next_source_id += 1;
    let source = Source {
        id: source_id,
        name: "Image".to_string(),
        source_type: SourceType::Image,
        properties: SourceProperties::Image { path: String::new() },
        transform: Transform { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    state.sources.push(source);
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
        scene.sources.push(source_id);
    }
    state.selected_source_id = Some(source_id);
    state.scenes_dirty = true;
    // No capture command — image loads from Properties panel
}
```

Wire it in the match: `SourceType::Image => add_image_source(state, &cmd_tx, active_id),`

- [ ] **Step 3: Guard remove_source for image types**

In `remove_source()`, only send `RemoveCaptureSource` for source types that use capture pipelines. Wrap the existing `tx.try_send(GstCommand::RemoveCaptureSource { .. })` with:

```rust
let needs_capture_removal = !matches!(
    state.sources.iter().find(|s| s.id == src_id).map(|s| &s.source_type),
    Some(SourceType::Image)
);
if needs_capture_removal {
    if let Some(tx) = cmd_tx { ... }
}
```

- [ ] **Step 4: Add Image properties UI**

In `src/ui/properties_panel.rs`, replace the Image placeholder arm with:

```rust
SourceProperties::Image { path } => {
    ui.horizontal(|ui| {
        // Path display
        let mut path_buf = path.clone();
        let response = ui.add(
            egui::TextEdit::singleline(&mut path_buf)
                .desired_width(ui.available_width() - 120.0)
                .hint_text("Select an image..."),
        );
        if response.changed() {
            *path = path_buf;
        }

        // Browse button
        if ui.button(egui_phosphor::regular::FOLDER_OPEN).on_hover_text("Browse").clicked() {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"])
                .pick_file()
            {
                let file_path = file.to_string_lossy().to_string();
                match crate::image_source::load_image_source(&file_path) {
                    Ok(frame) => {
                        // Set transform to image native dimensions
                        if let Some(source) = state.sources.iter_mut().find(|s| s.id == src_id) {
                            source.transform.width = frame.width as f32;
                            source.transform.height = frame.height as f32;
                        }
                        // Send frame via command channel
                        if let Some(tx) = &cmd_tx {
                            let _ = tx.try_send(GstCommand::LoadImageFrame { source_id: src_id, frame });
                        }
                        *path = file_path;
                    }
                    Err(e) => {
                        state.active_errors.push(GstError::CaptureFailure {
                            message: format!("Failed to load image: {e}"),
                        });
                    }
                }
                state.scenes_dirty = true;
            }
        }

        // Reload button
        if !path.is_empty() {
            if ui.button(egui_phosphor::regular::ARROW_CLOCKWISE).on_hover_text("Reload").clicked() {
                match crate::image_source::load_image_source(path) {
                    Ok(frame) => {
                        if let Some(tx) = &cmd_tx {
                            let _ = tx.try_send(GstCommand::LoadImageFrame { source_id: src_id, frame });
                        }
                    }
                    Err(e) => {
                        state.active_errors.push(GstError::CaptureFailure {
                            message: format!("Failed to reload image: {e}"),
                        });
                    }
                }
            }
        }
    });
}
```

Note: The implementer will need to handle borrow checker issues — clone `cmd_tx` and `src_id` before entering the mutable borrow on source properties. Check `egui_phosphor::regular::FOLDER_OPEN` exists — if not, use `FOLDER` or a text label.

- [ ] **Step 5: Build and verify**

Run: `cargo build`

- [ ] **Step 6: Commit**

```bash
git add src/ui/sources_panel.rs src/ui/properties_panel.rs
git commit -m "feat(ui): add image source type picker and properties with browse/reload"
```

---

### Task 4: Final Integration

**Files:**
- All modified files

- [ ] **Step 1: Build**

Run: `cargo build`

- [ ] **Step 2: Test**

Run: `cargo test`

- [ ] **Step 3: Clippy + fmt**

Run: `cargo clippy && cargo fmt --check`
Fix any issues.

- [ ] **Step 4: Commit fixes**

```bash
git add -A
git commit -m "chore: final integration fixes for image sources"
```
