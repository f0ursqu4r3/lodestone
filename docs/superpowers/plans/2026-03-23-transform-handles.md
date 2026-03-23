# Interactive Transform Handles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add click-drag transform handles in the preview panel so users can visually reposition and resize sources.

**Architecture:** A new `transform_handles.rs` module handles all drawing, hit testing, and drag logic. The preview panel calls it after rendering the preview. Handles appear for the source selected in `state.selected_source_id`. Coordinate mapping converts between canvas space (source transforms) and screen space (egui viewport).

**Tech Stack:** Rust, egui (drawing + input)

**Spec:** `docs/superpowers/specs/2026-03-23-transform-handles-design.md`

---

## File Structure

```
src/ui/transform_handles.rs  # NEW — handle drawing, hit testing, drag logic, coord mapping
src/ui/preview_panel.rs      # MODIFY — call draw_transform_handles
src/ui/mod.rs                # MODIFY — add transform_handles module
```

---

### Task 1: Coordinate Mapping and Data Types

**Files:**
- Create: `src/ui/transform_handles.rs`
- Modify: `src/ui/mod.rs` (add module)

- [ ] **Step 1: Create `src/ui/transform_handles.rs` with types and coordinate mapping**

```rust
//! Interactive transform handles for repositioning and resizing sources in the preview.

use egui::{Pos2, Rect, Vec2};
use crate::state::AppState;
use crate::scene::Transform;

/// Which handle is being interacted with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HandlePosition {
    TopLeft, Top, TopRight,
    Left, Right,
    BottomLeft, Bottom, BottomRight,
}

/// Current drag interaction state.
#[derive(Clone, Debug)]
enum DragMode {
    None,
    Move {
        start_mouse: Pos2,
        start_transform: Transform,
    },
    Resize {
        handle: HandlePosition,
        start_mouse: Pos2,
        start_transform: Transform,
        aspect_ratio: f32,
    },
}

/// Convert a canvas-space position to screen-space within the viewport.
fn canvas_to_screen(canvas_pos: Pos2, viewport: Rect, canvas_size: Vec2) -> Pos2 {
    let scale_x = viewport.width() / canvas_size.x;
    let scale_y = viewport.height() / canvas_size.y;
    Pos2::new(
        viewport.min.x + canvas_pos.x * scale_x,
        viewport.min.y + canvas_pos.y * scale_y,
    )
}

/// Convert a screen-space position back to canvas-space.
fn screen_to_canvas(screen_pos: Pos2, viewport: Rect, canvas_size: Vec2) -> Pos2 {
    let scale_x = canvas_size.x / viewport.width();
    let scale_y = canvas_size.y / viewport.height();
    Pos2::new(
        (screen_pos.x - viewport.min.x) * scale_x,
        (screen_pos.y - viewport.min.y) * scale_y,
    )
}

/// Get the screen-space rect for a source transform.
fn transform_to_screen_rect(t: &Transform, viewport: Rect, canvas_size: Vec2) -> Rect {
    let min = canvas_to_screen(Pos2::new(t.x, t.y), viewport, canvas_size);
    let max = canvas_to_screen(Pos2::new(t.x + t.width, t.y + t.height), viewport, canvas_size);
    Rect::from_min_max(min, max)
}
```

- [ ] **Step 2: Add module to `src/ui/mod.rs`**

Add `pub mod transform_handles;`

- [ ] **Step 3: Write tests for coordinate mapping**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_to_screen_maps_origin() {
        let viewport = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(960.0, 540.0));
        let canvas = Vec2::new(1920.0, 1080.0);
        let result = canvas_to_screen(Pos2::new(0.0, 0.0), viewport, canvas);
        assert!((result.x - 100.0).abs() < 0.01);
        assert!((result.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn screen_to_canvas_roundtrip() {
        let viewport = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::new(960.0, 540.0));
        let canvas = Vec2::new(1920.0, 1080.0);
        let original = Pos2::new(480.0, 270.0);
        let screen = canvas_to_screen(original, viewport, canvas);
        let back = screen_to_canvas(screen, viewport, canvas);
        assert!((back.x - original.x).abs() < 0.1);
        assert!((back.y - original.y).abs() < 0.1);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test transform_handles`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/transform_handles.rs src/ui/mod.rs
git commit -m "feat(ui): add transform handle types and coordinate mapping"
```

---

### Task 2: Handle Drawing

**Files:**
- Modify: `src/ui/transform_handles.rs` (add drawing functions)

- [ ] **Step 1: Add handle drawing**

Add constants and a drawing function:

```rust
use crate::ui::theme::{TEXT_PRIMARY, TEXT_SECONDARY, BG_BASE, BORDER};

const CORNER_SIZE: f32 = 8.0;
const EDGE_SIZE: f32 = 6.0;
const CORNER_HIT_SIZE: f32 = 16.0;
const EDGE_HIT_SIZE: f32 = 12.0;
const MIN_SOURCE_SIZE: f32 = 10.0;

/// Draw the selection outline and handles for a source rect.
fn draw_handles(painter: &egui::Painter, screen_rect: Rect) {
    // Selection outline
    painter.rect_stroke(
        screen_rect,
        0.0,
        egui::Stroke::new(1.0, TEXT_PRIMARY),
        egui::StrokeKind::Outside,
    );

    // Corner handles
    for pos in corner_positions(screen_rect) {
        let handle_rect = Rect::from_center_size(pos, Vec2::splat(CORNER_SIZE));
        painter.rect_filled(handle_rect, 1.0, TEXT_PRIMARY);
        painter.rect_stroke(handle_rect, 1.0, egui::Stroke::new(1.0, BG_BASE), egui::StrokeKind::Outside);
    }

    // Edge handles
    for pos in edge_positions(screen_rect) {
        let handle_rect = Rect::from_center_size(pos, Vec2::splat(EDGE_SIZE));
        painter.rect_filled(handle_rect, 1.0, TEXT_PRIMARY);
        painter.rect_stroke(handle_rect, 1.0, egui::Stroke::new(1.0, BG_BASE), egui::StrokeKind::Outside);
    }
}

fn corner_positions(r: Rect) -> [Pos2; 4] {
    [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()]
}

fn edge_positions(r: Rect) -> [Pos2; 4] {
    [
        Pos2::new(r.center().x, r.top()),    // Top
        Pos2::new(r.center().x, r.bottom()), // Bottom
        Pos2::new(r.left(), r.center().y),    // Left
        Pos2::new(r.right(), r.center().y),   // Right
    ]
}
```

- [ ] **Step 2: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "feat(ui): add transform handle drawing"
```

---

### Task 3: Hit Testing and Drag Logic

**Files:**
- Modify: `src/ui/transform_handles.rs` (add hit testing, drag state, input handling)

- [ ] **Step 1: Add hit testing**

```rust
/// Determine which handle (if any) a screen-space point is over.
fn hit_test_handles(pos: Pos2, screen_rect: Rect) -> Option<HandlePosition> {
    let corners = corner_positions(screen_rect);
    let corner_handles = [
        HandlePosition::TopLeft, HandlePosition::TopRight,
        HandlePosition::BottomLeft, HandlePosition::BottomRight,
    ];
    for (i, &corner) in corners.iter().enumerate() {
        let hit = Rect::from_center_size(corner, Vec2::splat(CORNER_HIT_SIZE));
        if hit.contains(pos) {
            return Some(corner_handles[i]);
        }
    }

    let edges = edge_positions(screen_rect);
    let edge_handles = [
        HandlePosition::Top, HandlePosition::Bottom,
        HandlePosition::Left, HandlePosition::Right,
    ];
    for (i, &edge) in edges.iter().enumerate() {
        let hit = Rect::from_center_size(edge, Vec2::splat(EDGE_HIT_SIZE));
        if hit.contains(pos) {
            return Some(edge_handles[i]);
        }
    }

    None
}
```

- [ ] **Step 2: Add the main entry point with full drag logic**

```rust
/// Draw transform handles and process drag interactions for the selected source.
pub fn draw_transform_handles(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewport_rect: Rect,
    canvas_size: Vec2,
) {
    let Some(selected_id) = state.selected_source_id else { return };
    let Some(source) = state.sources.iter().find(|s| s.id == selected_id) else { return };
    let transform = source.transform.clone();

    let screen_rect = transform_to_screen_rect(&transform, viewport_rect, canvas_size);

    // Draw handles
    let painter = ui.painter();
    draw_handles(painter, screen_rect);

    // Get drag state from egui memory
    let drag_id = egui::Id::new(("transform_drag", selected_id));
    let mut drag_mode: DragMode = ui.memory(|m| {
        m.data.get_temp(drag_id).unwrap_or(DragMode::None)
    });

    let pointer = ui.input(|i| i.pointer.hover_pos());
    let primary_down = ui.input(|i| i.pointer.primary_down());
    let primary_released = ui.input(|i| i.pointer.primary_released());
    let shift_held = ui.input(|i| i.modifiers.shift);

    if let Some(mouse_pos) = pointer {
        match &drag_mode {
            DragMode::None => {
                if primary_down && viewport_rect.contains(mouse_pos) {
                    // Check handles first, then body
                    if let Some(handle) = hit_test_handles(mouse_pos, screen_rect) {
                        drag_mode = DragMode::Resize {
                            handle,
                            start_mouse: mouse_pos,
                            start_transform: transform.clone(),
                            aspect_ratio: transform.width / transform.height.max(1.0),
                        };
                    } else if screen_rect.contains(mouse_pos) {
                        drag_mode = DragMode::Move {
                            start_mouse: mouse_pos,
                            start_transform: transform.clone(),
                        };
                    }
                }
            }
            DragMode::Move { start_mouse, start_transform } => {
                let delta = screen_to_canvas(mouse_pos, viewport_rect, canvas_size)
                    - screen_to_canvas(*start_mouse, viewport_rect, canvas_size);
                if let Some(source) = state.sources.iter_mut().find(|s| s.id == selected_id) {
                    source.transform.x = start_transform.x + delta.x;
                    source.transform.y = start_transform.y + delta.y;
                }
            }
            DragMode::Resize { handle, start_mouse, start_transform, aspect_ratio } => {
                let delta = screen_to_canvas(mouse_pos, viewport_rect, canvas_size)
                    - screen_to_canvas(*start_mouse, viewport_rect, canvas_size);
                if let Some(source) = state.sources.iter_mut().find(|s| s.id == selected_id) {
                    apply_resize(&mut source.transform, start_transform, *handle, delta, shift_held, *aspect_ratio);
                }
            }
        }

        if primary_released && !matches!(drag_mode, DragMode::None) {
            state.scenes_dirty = true;
            drag_mode = DragMode::None;
        }
    }

    ui.memory_mut(|m| m.data.insert_temp(drag_id, drag_mode));
}
```

The implementer will need to implement `apply_resize()` which computes the new transform based on which handle is dragged, the delta, and whether Shift constrains the aspect ratio. The logic: the opposite corner/edge stays pinned, the dragged corner/edge moves by the delta, and the transform x/y/width/height are recalculated. Enforce minimum size of `MIN_SOURCE_SIZE`.

- [ ] **Step 3: Implement apply_resize**

```rust
fn apply_resize(
    transform: &mut Transform,
    start: &Transform,
    handle: HandlePosition,
    delta: Vec2,
    constrain_aspect: bool,
    aspect_ratio: f32,
) {
    let (mut x, mut y, mut w, mut h) = (start.x, start.y, start.width, start.height);

    match handle {
        HandlePosition::TopLeft => { x += delta.x; y += delta.y; w -= delta.x; h -= delta.y; }
        HandlePosition::Top => { y += delta.y; h -= delta.y; }
        HandlePosition::TopRight => { y += delta.y; w += delta.x; h -= delta.y; }
        HandlePosition::Left => { x += delta.x; w -= delta.x; }
        HandlePosition::Right => { w += delta.x; }
        HandlePosition::BottomLeft => { x += delta.x; w -= delta.x; h += delta.y; }
        HandlePosition::Bottom => { h += delta.y; }
        HandlePosition::BottomRight => { w += delta.x; h += delta.y; }
    }

    // Aspect ratio constraint (Shift held, corners only)
    if constrain_aspect && matches!(handle,
        HandlePosition::TopLeft | HandlePosition::TopRight |
        HandlePosition::BottomLeft | HandlePosition::BottomRight
    ) {
        h = w / aspect_ratio;
        // Re-pin for top handles
        if matches!(handle, HandlePosition::TopLeft | HandlePosition::TopRight) {
            y = start.y + start.height - h;
        }
    }

    // Enforce minimum size
    w = w.max(MIN_SOURCE_SIZE);
    h = h.max(MIN_SOURCE_SIZE);

    transform.x = x;
    transform.y = y;
    transform.width = w;
    transform.height = h;
}
```

- [ ] **Step 4: Build and verify**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "feat(ui): add hit testing, drag logic, and resize for transform handles"
```

---

### Task 4: Wire into Preview Panel

**Files:**
- Modify: `src/ui/preview_panel.rs`

- [ ] **Step 1: Call draw_transform_handles from the preview panel**

In `preview_panel.rs`, after the existing overlay drawing (LIVE badge, resolution overlay), add:

```rust
// Transform handles for selected source
let canvas_size = egui::Vec2::new(
    state.settings.video.base_resolution_width() as f32,
    state.settings.video.base_resolution_height() as f32,
);
// Use the letterboxed preview_rect as the viewport
crate::ui::transform_handles::draw_transform_handles(ui, state, preview_rect, canvas_size);
```

The implementer needs to find the correct `preview_rect` variable — it's the letterboxed viewport rect computed by `letterboxed_rect()`. And determine how to get canvas dimensions (may need to read from settings or use a constant).

Note: The `state` parameter is already `&mut AppState`, so passing it to `draw_transform_handles` should work without borrow issues.

- [ ] **Step 2: Build and verify**

Run: `cargo build`

- [ ] **Step 3: Commit**

```bash
git add src/ui/preview_panel.rs
git commit -m "feat(ui): wire transform handles into preview panel"
```

---

### Task 5: Final Integration

- [ ] **Step 1: Build, test, clippy, fmt**

Run: `cargo build && cargo test && cargo clippy && cargo fmt --check`
Fix any issues.

- [ ] **Step 2: Commit fixes**

```bash
git add -A
git commit -m "chore: final integration for transform handles"
```
