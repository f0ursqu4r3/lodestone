# Preview Pane Interaction Refinements — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the preview panel with magnetic snapping, grid/guides, zoom/pan, copy/paste, multi-select, nudge, rotation, z-order shortcuts, snap toggle, and source locking.

**Architecture:** All features layer onto the existing `transform_handles.rs` and `preview_panel.rs` modules, with data model changes in `scene.rs`, `state.rs`, and `settings.rs`. The plan is ordered so each task builds on the previous — data model first, then interaction logic, then keyboard shortcuts.

**Tech Stack:** Rust, egui, wgpu, winit, serde/toml

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/scene.rs` | Add `rotation` to Transform, `Guide` struct, `Vec<Guide>` on Scene, `locked` on SourceOverrides |
| `src/state.rs` | Multi-select model (`selected_source_ids`, `primary_selected_id`), clipboard (`Vec<ClipboardEntry>`), nudge batch timer |
| `src/settings.rs` | Grid presets, grid/guide colors, safe zone toggles, rule-of-thirds toggle, guide visibility |
| `src/ui/transform_handles.rs` | Magnetic snap, rotation handles, multi-select interaction, lock dimming, marquee select, snap toggle |
| `src/ui/preview_panel.rs` | Zoom/pan state, grid/guide rendering, rulers, zoom badges, pan interaction |
| `src/ui/sources_panel.rs` | Lock icon per source row |
| `src/main.rs` | All new keyboard shortcuts (copy/paste/duplicate, select all, lock, z-order, nudge, zoom reset) |

---

## Task 1: Data Model — Transform Rotation

**Files:**
- Modify: `src/scene.rs:87-92` (Transform struct)
- Modify: `src/scene.rs:354-363` (Transform::new)

- [ ] **Step 1: Add `rotation` field to Transform**

In `src/scene.rs`, add `rotation: f32` to the `Transform` struct with `#[serde(default)]` so existing TOML files load without breaking:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation: f32, // Degrees, default 0.0
}
```

Update `Transform::new` to set `rotation: 0.0`.

- [ ] **Step 2: Verify build**

Run: `cargo build 2>&1 | head -20`
Expected: Compiles. Any errors will be about missing `rotation` field in struct literals — fix those.

- [ ] **Step 3: Fix all struct literal construction sites**

Search for `Transform {` and `Transform::new(` across the codebase. Add `rotation: 0.0` to any struct literal that doesn't use `..` spread. Likely locations:
- `src/ui/transform_handles.rs` (context menu actions: Fit, Stretch, Fill, Center, Reset)
- `src/scene.rs` (default construction)

- [ ] **Step 4: Verify build and tests pass**

Run: `cargo build && cargo test`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add src/scene.rs src/ui/transform_handles.rs
git commit -m "feat: add rotation field to Transform"
```

---

## Task 2: Data Model — Guide, Locked, Multi-Select, Clipboard

**Files:**
- Modify: `src/scene.rs:13-20` (Scene struct), `src/scene.rs:61-72` (SourceOverrides)
- Modify: `src/state.rs:95-140` (AppState)
- Modify: `src/settings.rs:55-80` (GeneralSettings)

- [ ] **Step 1: Add Guide struct and Vec<Guide> to Scene**

In `src/scene.rs`, add above the `Scene` struct:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GuideAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Guide {
    pub axis: GuideAxis,
    pub position: f32, // Canvas-space coordinate
}
```

Add to `Scene`:

```rust
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    pub sources: Vec<SceneSource>,
    pub pinned: bool,
    #[serde(default)]
    pub guides: Vec<Guide>,
}
```

- [ ] **Step 2: Add `locked` to SourceOverrides**

```rust
pub struct SourceOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
}
```

Add a `resolve_locked` method to `SceneSource`:

```rust
pub fn resolve_locked(&self) -> bool {
    self.overrides.locked.unwrap_or(false)
}
```

- [ ] **Step 3: Update AppState for multi-select and clipboard**

In `src/state.rs`, replace selection fields and add clipboard:

```rust
// Replace:
//   pub selected_source_id: Option<SourceId>,
// With:
pub selected_source_ids: Vec<SourceId>,
pub primary_selected_id: Option<SourceId>,

// Add clipboard:
pub clipboard: Vec<ClipboardEntry>,
```

Add the ClipboardEntry struct near the top of `state.rs`:

```rust
#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    pub library_source_id: SourceId,
    pub overrides_snapshot: SourceOverrides,
}
```

- [ ] **Step 4: Add helper methods on AppState for selection**

Add backward-compatible helpers so existing code can migrate incrementally:

```rust
/// Returns the primary selected source ID (backward compat).
pub fn selected_source_id(&self) -> Option<SourceId> {
    self.primary_selected_id
}

/// Select a single source (clears multi-select).
pub fn select_source(&mut self, id: SourceId) {
    self.selected_source_ids = vec![id];
    self.primary_selected_id = Some(id);
}

/// Deselect all sources.
pub fn deselect_all(&mut self) {
    self.selected_source_ids.clear();
    self.primary_selected_id = None;
}

/// Toggle a source in the selection (for Shift+click).
pub fn toggle_source_selection(&mut self, id: SourceId) {
    if let Some(pos) = self.selected_source_ids.iter().position(|&s| s == id) {
        self.selected_source_ids.remove(pos);
        if self.primary_selected_id == Some(id) {
            self.primary_selected_id = self.selected_source_ids.last().copied();
        }
    } else {
        self.selected_source_ids.push(id);
        self.primary_selected_id = Some(id);
    }
}

/// Check if a source is selected.
pub fn is_source_selected(&self, id: SourceId) -> bool {
    self.selected_source_ids.contains(&id)
}
```

- [ ] **Step 5: Fix all compilation errors from selection model change**

Search the codebase for `selected_source_id` (the old field). Replace reads with `state.selected_source_id()` (the new helper method) and writes with `state.select_source(id)` or `state.deselect_all()`. Key locations:
- `src/ui/transform_handles.rs` — selection clicks, drag logic
- `src/ui/preview_panel.rs` — drop acceptance
- `src/ui/sources_panel.rs` — row selection, delete
- `src/ui/properties_panel.rs` — reading selected source
- `src/main.rs` — delete handler

- [ ] **Step 6: Update UndoStack::restore to handle new fields**

In the `restore` function, clear `selected_source_ids` and `primary_selected_id` if the restored state doesn't contain the selected sources:

```rust
// After restoring scenes/library, validate selections
state.selected_source_ids.retain(|id| {
    state.active_scene()
        .map(|s| s.find_source(*id).is_some())
        .unwrap_or(false)
});
if let Some(primary) = state.primary_selected_id {
    if !state.selected_source_ids.contains(&primary) {
        state.primary_selected_id = state.selected_source_ids.last().copied();
    }
}
```

- [ ] **Step 7: Add new settings fields**

In `src/settings.rs`, extend `GeneralSettings`:

```rust
pub struct GeneralSettings {
    // ... existing fields ...
    pub snap_to_grid: bool,
    pub snap_grid_size: f32,
    #[serde(default)]
    pub grid_preset: String,              // "custom", "8", "16", "32", "64", "thirds", "quarters"
    #[serde(default)]
    pub show_grid: bool,                  // visible grid overlay
    #[serde(default)]
    pub show_guides: bool,                // custom guides visible
    #[serde(default)]
    pub show_thirds: bool,                // rule-of-thirds overlay
    #[serde(default)]
    pub show_safe_zones: bool,            // action-safe/title-safe
    #[serde(default = "default_grid_color")]
    pub grid_color: [u8; 3],              // [255, 255, 255]
    #[serde(default = "default_grid_opacity")]
    pub grid_opacity: f32,                // 0.15
    #[serde(default = "default_guide_color")]
    pub guide_color: [u8; 3],             // [0, 255, 255]
    #[serde(default = "default_guide_opacity")]
    pub guide_opacity: f32,               // 0.60
    // ... existing fields ...
}
```

Add the default functions:

```rust
fn default_grid_color() -> [u8; 3] { [255, 255, 255] }
fn default_grid_opacity() -> f32 { 0.15 }
fn default_guide_color() -> [u8; 3] { [0, 255, 255] }
fn default_guide_opacity() -> f32 { 0.60 }
```

- [ ] **Step 8: Verify build and tests pass**

Run: `cargo build && cargo test`
Expected: All pass.

- [ ] **Step 9: Commit**

```bash
git add src/scene.rs src/state.rs src/settings.rs src/ui/transform_handles.rs src/ui/preview_panel.rs src/ui/sources_panel.rs src/ui/properties_panel.rs src/main.rs
git commit -m "feat: add data model for guides, locking, multi-select, and clipboard"
```

---

## Task 3: Magnetic Snapping

**Files:**
- Modify: `src/ui/transform_handles.rs:30-43` (DragMode), `src/ui/transform_handles.rs:220-316` (snap logic)

- [ ] **Step 1: Add snap tracking to DragMode::Move**

Extend `DragMode::Move` to track magnetic snap state:

```rust
enum DragMode {
    None,
    Move {
        start_mouse: Pos2,
        start_transform: Transform,
        raw_x: f32,         // Unsnapped X position (tracks true mouse intent)
        raw_y: f32,         // Unsnapped Y position
        snapped_x: Option<f32>,  // Snap target X (None = not snapped)
        snapped_y: Option<f32>,  // Snap target Y (None = not snapped)
    },
    Resize {
        handle: HandlePosition,
        start_mouse: Pos2,
        start_transform: Transform,
        aspect_ratio: f32,
    },
}
```

- [ ] **Step 2: Add snap constants**

Replace `SNAP_THRESHOLD` with two constants:

```rust
const SNAP_ATTRACT: f32 = 12.0;  // Outer zone: pull source in
const SNAP_DEAD: f32 = 4.0;      // Inner zone: resist leaving
```

- [ ] **Step 3: Rewrite snap_transform to return snap info instead of mutating**

Create a new function that returns snap results without mutating the transform:

```rust
struct SnapResult {
    snapped_x: Option<f32>,   // The target X value (None = no snap)
    snapped_y: Option<f32>,   // The target Y value
    offset_x: f32,            // Delta to apply to transform.x
    offset_y: f32,            // Delta to apply to transform.y
}

fn compute_snap(
    transform: &Transform,
    canvas_size: Vec2,
    grid_size: f32,
    other_sources: &[&Transform],
    guides: &[Guide],
    show_thirds: bool,
    show_safe_zones: bool,
    attract: f32,
) -> SnapResult
```

This function builds snap targets from all sources: canvas edges, center, grid, other sources, custom guides, thirds lines (if enabled), safe zone boundaries (if enabled). Returns the best snap per axis.

- [ ] **Step 4: Implement magnetic snap logic in the move handler**

In the `DragMode::Move` arm of the drag handling code:

```rust
// 1. Compute raw position from mouse delta
let delta_canvas = screen_to_canvas(current_mouse, viewport, canvas_size)
    - screen_to_canvas(drag.start_mouse, viewport, canvas_size);
let raw_x = drag.start_transform.x + delta_canvas.x;
let raw_y = drag.start_transform.y + delta_canvas.y;

// Store raw position
drag.raw_x = raw_x;
drag.raw_y = raw_y;

// 2. Check if Alt is held (suppress snapping)
let suppress_snap = ui.input(|i| i.modifiers.alt);

if suppress_snap || !state.settings.general.snap_to_grid {
    // No snapping — use raw position directly
    transform.x = raw_x;
    transform.y = raw_y;
    drag.snapped_x = None;
    drag.snapped_y = None;
} else {
    // 3. Magnetic snap logic per axis
    // X axis
    if let Some(snap_line) = drag.snapped_x {
        // Currently snapped — check dead zone
        if (raw_x - (snap_line - transform.width * offset_factor_x)).abs() > SNAP_DEAD {
            // Broke free
            drag.snapped_x = None;
            transform.x = raw_x;
        } else {
            // Stay locked
            transform.x = snap_line; // (adjusted for which edge snapped)
        }
    } else {
        // Not snapped — check attraction zone
        let snap = compute_snap(&Transform { x: raw_x, y: raw_y, ..transform }, ...);
        if let Some(sx) = snap.snapped_x {
            drag.snapped_x = Some(sx);
            transform.x = raw_x + snap.offset_x;
        } else {
            transform.x = raw_x;
        }
    }
    // Y axis: same pattern
}
```

Note: The above is pseudocode showing the pattern. The actual implementation must track which edge (left/right/center) snapped so the dead zone comparison uses the correct reference point.

- [ ] **Step 5: Scale snap zones with zoom**

Where snap zones are used, multiply by `1.0 / zoom` so snapping feels consistent at all zoom levels. The `zoom` value will come from preview panel state (added in Task 5). For now, default to `1.0`:

```rust
let scale = 1.0 / zoom;
let attract = SNAP_ATTRACT * scale;
let dead = SNAP_DEAD * scale;
```

- [ ] **Step 6: Verify build and manual test**

Run: `cargo build && cargo run`
Test: Drag a source near canvas edge — it should snap and resist leaving until you drag forcefully past the dead zone.

- [ ] **Step 7: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "feat: implement magnetic two-zone snapping"
```

---

## Task 4: Multi-Select Interaction

**Files:**
- Modify: `src/ui/transform_handles.rs:393-738` (draw_transform_handles)

- [ ] **Step 1: Update click-to-select for multi-select**

In the click handling section of `draw_transform_handles` (around line 419-490), update logic:

```rust
// On primary click:
let shift = ui.input(|i| i.modifiers.shift);
if let Some(hit_source_id) = hit_source {
    if shift {
        state.toggle_source_selection(hit_source_id);
    } else {
        state.select_source(hit_source_id);
    }
} else {
    // Clicked empty space — start marquee or deselect
    if !shift {
        state.deselect_all();
    }
}
```

- [ ] **Step 2: Implement marquee selection**

Add a new `DragMode::Marquee` variant:

```rust
Marquee {
    start: Pos2,  // Screen-space start point
},
```

When clicking empty space (no source hit), enter `DragMode::Marquee`. During drag, draw a selection rectangle. On release, select all sources whose screen rects intersect the marquee rect:

```rust
DragMode::Marquee { start } => {
    let current = ui.input(|i| i.pointer.interact_pos().unwrap_or(*start));
    let marquee = Rect::from_two_pos(*start, current);

    // Draw marquee rectangle
    let stroke = Stroke::new(1.0, state.accent_color);
    let fill = state.accent_color.linear_multiply(0.1);
    painter.rect(marquee, 0.0, fill, stroke);

    if ui.input(|i| i.pointer.primary_released()) {
        let shift = ui.input(|i| i.modifiers.shift);
        if !shift {
            state.selected_source_ids.clear();
        }
        // Hit test all visible sources against marquee
        for (source_id, screen_rect) in &visible_source_rects {
            let locked = /* resolve locked for this source */;
            if !locked && marquee.intersects(*screen_rect) {
                if !state.selected_source_ids.contains(source_id) {
                    state.selected_source_ids.push(*source_id);
                }
            }
        }
        state.primary_selected_id = state.selected_source_ids.last().copied();
        // Reset drag mode
    }
}
```

- [ ] **Step 3: Update move drag to move all selected sources**

When starting a move drag on a selected source, capture start transforms for ALL selected sources. During drag, apply the same canvas-space delta to all:

```rust
Move {
    start_mouse: Pos2,
    start_transforms: Vec<(SourceId, Transform)>,  // All selected sources
    anchor_id: SourceId,                            // Source being directly dragged
    raw_x: f32,
    raw_y: f32,
    snapped_x: Option<f32>,
    snapped_y: Option<f32>,
}
```

During drag, compute delta from anchor source, apply to all. Snapping applies to the anchor only — other sources follow with the same offset.

- [ ] **Step 4: Draw selection outlines for all selected sources**

After the main drag handling, draw selection outlines for all selected sources. Only draw handles on the primary selected source:

```rust
for &id in &state.selected_source_ids {
    let screen_rect = /* compute screen rect for source id */;
    // Draw selection outline
    painter.rect_stroke(screen_rect, 0.0, Stroke::new(1.0, TEXT_PRIMARY));

    // Only draw handles on primary
    if Some(id) == state.primary_selected_id {
        draw_handles(&painter, screen_rect);
    }
}
```

- [ ] **Step 5: Verify build and test**

Run: `cargo build && cargo run`
Test: Shift+click multiple sources, drag to move as group, marquee select on empty space.

- [ ] **Step 6: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "feat: implement multi-select with marquee and group move"
```

---

## Task 5: Zoom & Pan

**Files:**
- Modify: `src/ui/preview_panel.rs:84-249` (draw_inner)
- Modify: `src/ui/transform_handles.rs:47-69` (coordinate mapping)

- [ ] **Step 1: Add zoom/pan state**

Add ephemeral zoom/pan state to a struct stored in egui's temporary memory (not on AppState — these are per-session, not persisted):

```rust
#[derive(Clone)]
struct PreviewViewState {
    zoom: f32,           // Multiplier on fit-to-panel. 1.0 = fit canvas to panel.
    pan_offset: Vec2,    // Canvas-space offset from center
    space_held: bool,    // Spacebar hand-tool mode
}

impl Default for PreviewViewState {
    fn default() -> Self {
        Self { zoom: 1.0, pan_offset: Vec2::ZERO, space_held: false }
    }
}
```

Store/retrieve via `ui.data_mut(|d| d.get_temp_mut_or_default::<PreviewViewState>(Id::new("preview_view")))`.

- [ ] **Step 2: Update letterboxing to apply zoom and pan**

Replace the simple `letterboxed_rect` call with a zoom/pan-aware viewport computation:

```rust
fn zoomed_viewport(panel: Rect, canvas_w: u32, canvas_h: u32, zoom: f32, pan: Vec2) -> Rect {
    // Base: fit canvas to panel (letterboxed)
    let base = letterboxed_rect(panel, canvas_w, canvas_h);
    let base_size = base.size();

    // Apply zoom: scale around panel center
    let zoomed_size = base_size * zoom;

    // Center + pan offset (pan is in canvas space, convert to screen)
    let pixels_per_canvas = zoomed_size.x / canvas_w as f32;
    let screen_pan = pan * pixels_per_canvas;

    let center = panel.center() + screen_pan;
    Rect::from_center_size(center, zoomed_size)
}
```

Use this `zoomed_viewport` rect as the viewport passed to `draw_transform_handles` and the GPU callback. Clip rendering to `panel_rect` to prevent drawing outside the panel.

- [ ] **Step 3: Handle scroll wheel zoom**

In `draw_inner`, before allocating space, handle scroll input:

```rust
let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
if scroll_delta != 0.0 && panel_rect.contains(ui.input(|i| i.pointer.hover_pos().unwrap_or_default())) {
    let cursor_pos = ui.input(|i| i.pointer.hover_pos().unwrap_or(panel_rect.center()));
    let old_zoom = view.zoom;

    // Stepped zoom for scroll wheel
    let steps = [0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];
    let current_idx = steps.iter().position(|&s| s >= old_zoom).unwrap_or(steps.len() - 1);
    let new_zoom = if scroll_delta > 0.0 {
        steps.get(current_idx + 1).copied().unwrap_or(8.0)
    } else {
        if current_idx > 0 { steps[current_idx - 1] } else { 0.1 }
    };

    // Adjust pan to keep cursor position stable
    let cursor_canvas = screen_to_canvas(cursor_pos, old_viewport, canvas_size);
    view.zoom = new_zoom;
    // Recompute viewport, then adjust pan so cursor_canvas maps back to cursor_pos
    // (cursor-centered zoom)
    let new_viewport = zoomed_viewport(panel_rect, canvas_w, canvas_h, view.zoom, view.pan_offset);
    let cursor_screen_after = canvas_to_screen(cursor_canvas, new_viewport, canvas_size);
    let correction = cursor_pos - cursor_screen_after;
    view.pan_offset += correction / (new_viewport.width() / canvas_w as f32);
}
```

- [ ] **Step 4: Handle trackpad pinch zoom**

egui exposes pinch gestures via `ui.input(|i| i.multi_touch())`. Use the zoom delta for continuous zoom:

```rust
if let Some(touch) = ui.input(|i| i.multi_touch()) {
    if touch.zoom_delta != 1.0 {
        let cursor_pos = touch.translation + panel_rect.center().to_vec2(); // approximate
        let old_zoom = view.zoom;
        view.zoom = (old_zoom * touch.zoom_delta).clamp(0.1, 8.0);
        // Same cursor-centered pan adjustment as scroll wheel
    }
}
```

- [ ] **Step 5: Handle pan (middle-click drag and spacebar+drag)**

```rust
// Spacebar tracking
if ui.input(|i| i.key_pressed(egui::Key::Space)) {
    view.space_held = true;
}
if ui.input(|i| i.key_released(egui::Key::Space)) {
    view.space_held = false;
}

// Middle-click drag or spacebar+left-click drag
let middle_dragging = ui.input(|i| i.pointer.middle_down());
let space_dragging = view.space_held && ui.input(|i| i.pointer.primary_down());

if middle_dragging || space_dragging {
    let drag_delta = ui.input(|i| i.pointer.delta());
    let pixels_per_canvas = viewport.width() / canvas_size.x;
    view.pan_offset += drag_delta / pixels_per_canvas;

    // Change cursor to grab hand
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
}

// Clamp pan so at least 10% of canvas visible
let max_pan_x = canvas_size.x * 0.9;
let max_pan_y = canvas_size.y * 0.9;
view.pan_offset.x = view.pan_offset.x.clamp(-max_pan_x, max_pan_x);
view.pan_offset.y = view.pan_offset.y.clamp(-max_pan_y, max_pan_y);
```

- [ ] **Step 6: Add zoom level to overlay badges**

In the resolution overlay (around line 142-165), append zoom percentage when zoomed:

```rust
let zoom_text = if view.zoom != 1.0 {
    format!(" · {:.0}%", view.zoom * 100.0) // relative to fit-to-panel
} else {
    String::new()
};
let text = format!("{}×{} · {}fps{}", preview_width, preview_height, fps, zoom_text);
```

- [ ] **Step 7: Update coordinate mapping to accept zoom/pan**

Update `canvas_to_screen` and `screen_to_canvas` in `transform_handles.rs` to use the zoomed viewport rect directly (no changes needed if the viewport rect passed already accounts for zoom/pan — which it does after Step 2).

Verify that handle hit areas use screen-space constants (CORNER_HIT_SIZE etc.) — these should NOT scale with zoom. Confirm this is already the case.

- [ ] **Step 8: Verify build and test**

Run: `cargo build && cargo run`
Test: Scroll to zoom (cursor-centered), middle-click drag to pan, spacebar+drag to pan, Cmd+0 fit-to-panel (Task 8 adds shortcuts).

- [ ] **Step 9: Commit**

```bash
git add src/ui/preview_panel.rs src/ui/transform_handles.rs
git commit -m "feat: implement zoom and pan on preview panel"
```

---

## Task 6: Grid & Guides Rendering

**Files:**
- Modify: `src/ui/preview_panel.rs` (grid/guide rendering)
- Modify: `src/ui/transform_handles.rs` (snap targets from guides/thirds/safe zones)

- [ ] **Step 1: Render visible grid overlay**

In `preview_panel.rs`, after the GPU callback and before transform handles, add grid rendering when `state.settings.general.show_grid` is true:

```rust
fn draw_grid(
    painter: &egui::Painter,
    viewport: Rect,
    canvas_size: Vec2,
    grid_size: f32,
    grid_preset: &str,
    color: Color32,
    clip_rect: Rect, // panel_rect, to clip grid to panel bounds
) {
    painter.set_clip_rect(clip_rect);

    // Compute grid lines based on preset
    let (x_lines, y_lines) = match grid_preset {
        "thirds" => {
            let xs = vec![canvas_size.x / 3.0, canvas_size.x * 2.0 / 3.0];
            let ys = vec![canvas_size.y / 3.0, canvas_size.y * 2.0 / 3.0];
            (xs, ys)
        }
        "quarters" => {
            let xs = vec![canvas_size.x * 0.25, canvas_size.x * 0.5, canvas_size.x * 0.75];
            let ys = vec![canvas_size.y * 0.25, canvas_size.y * 0.5, canvas_size.y * 0.75];
            (xs, ys)
        }
        _ => {
            // Pixel-based grid
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            let mut x = grid_size;
            while x < canvas_size.x {
                xs.push(x);
                x += grid_size;
            }
            let mut y = grid_size;
            while y < canvas_size.y {
                ys.push(y);
                y += grid_size;
            }
            (xs, ys)
        }
    };

    // Auto-hide when lines are too dense (< 4 screen pixels apart)
    let pixels_per_canvas = viewport.width() / canvas_size.x;
    let screen_spacing = grid_size * pixels_per_canvas;

    for (i, &x) in x_lines.iter().enumerate() {
        let is_major = (i + 1) % 4 == 0;
        if screen_spacing < 4.0 && !is_major { continue; }
        if screen_spacing < 4.0 && is_major && screen_spacing * 4.0 < 4.0 { continue; }

        let screen_x = canvas_to_screen(pos2(x, 0.0), viewport, canvas_size).x;
        let alpha = if is_major { color.a() } else { color.a() / 3 };
        let line_color = color.linear_multiply(alpha as f32 / 255.0);
        painter.line_segment(
            [pos2(screen_x, viewport.top()), pos2(screen_x, viewport.bottom())],
            Stroke::new(0.5, line_color),
        );
    }
    // Same for y_lines (horizontal lines)
}
```

- [ ] **Step 2: Render rule-of-thirds overlay**

When `state.settings.general.show_thirds` is true, draw the 4 rule-of-thirds lines in a distinct style (slightly thicker, different opacity):

```rust
fn draw_thirds(painter: &egui::Painter, viewport: Rect, canvas_size: Vec2, color: Color32) {
    let thirds_x = [canvas_size.x / 3.0, canvas_size.x * 2.0 / 3.0];
    let thirds_y = [canvas_size.y / 3.0, canvas_size.y * 2.0 / 3.0];
    let stroke = Stroke::new(1.0, color);
    for &x in &thirds_x {
        let sx = canvas_to_screen(pos2(x, 0.0), viewport, canvas_size).x;
        painter.line_segment([pos2(sx, viewport.top()), pos2(sx, viewport.bottom())], stroke);
    }
    for &y in &thirds_y {
        let sy = canvas_to_screen(pos2(0.0, y), viewport, canvas_size).y;
        painter.line_segment([pos2(viewport.left(), sy), pos2(viewport.right(), sy)], stroke);
    }
}
```

- [ ] **Step 3: Render safe zones**

When `state.settings.general.show_safe_zones` is true, draw action-safe (90%) and title-safe (80%) rectangles:

```rust
fn draw_safe_zones(painter: &egui::Painter, viewport: Rect, canvas_size: Vec2, color: Color32) {
    for factor in [0.9, 0.8] {
        let margin_x = canvas_size.x * (1.0 - factor) / 2.0;
        let margin_y = canvas_size.y * (1.0 - factor) / 2.0;
        let tl = canvas_to_screen(pos2(margin_x, margin_y), viewport, canvas_size);
        let br = canvas_to_screen(
            pos2(canvas_size.x - margin_x, canvas_size.y - margin_y),
            viewport, canvas_size,
        );
        let alpha = if factor == 0.9 { 0.4 } else { 0.3 };
        painter.rect_stroke(
            Rect::from_min_max(tl, br),
            0.0,
            Stroke::new(1.0, color.linear_multiply(alpha)),
        );
    }
}
```

- [ ] **Step 4: Render custom guides**

Draw per-scene custom guides as dashed colored lines:

```rust
fn draw_custom_guides(
    painter: &egui::Painter,
    viewport: Rect,
    canvas_size: Vec2,
    guides: &[Guide],
    color: Color32,
) {
    let stroke = Stroke::new(1.0, color);
    for guide in guides {
        match guide.axis {
            GuideAxis::Vertical => {
                let sx = canvas_to_screen(pos2(guide.position, 0.0), viewport, canvas_size).x;
                // Draw dashed line
                draw_dashed_line(painter, pos2(sx, viewport.top()), pos2(sx, viewport.bottom()), stroke, 6.0, 4.0);
            }
            GuideAxis::Horizontal => {
                let sy = canvas_to_screen(pos2(0.0, guide.position), viewport, canvas_size).y;
                draw_dashed_line(painter, pos2(viewport.left(), sy), pos2(viewport.right(), sy), stroke, 6.0, 4.0);
            }
        }
    }
}

fn draw_dashed_line(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke, dash: f32, gap: f32) {
    let dir = (to - from).normalized();
    let total = from.distance(to);
    let mut d = 0.0;
    while d < total {
        let end = (d + dash).min(total);
        painter.line_segment(
            [from + dir * d, from + dir * end],
            stroke,
        );
        d = end + gap;
    }
}
```

- [ ] **Step 5: Add guides, thirds, and safe zones as snap targets**

In `transform_handles.rs`, update `compute_snap` to include:
- Custom guide positions as snap targets (vertical guides → x_targets, horizontal → y_targets)
- Thirds lines when `show_thirds` is true
- Safe zone boundaries when `show_safe_zones` is true (90% and 80% margin positions)

Pass these from the preview panel into `draw_transform_handles` (extend the function signature or pass via a context struct).

- [ ] **Step 6: Implement guide creation by dragging from rulers**

Add thin ruler areas (12px) along the top and left edges of the preview area. On click+drag from a ruler into the canvas:
- Create a new `Guide` with the appropriate axis
- Add to `scene.guides`
- On right-click on a guide line, show a context menu with "Delete Guide"

This can be implemented by hit-testing guide lines (within 4px of the screen position) and using egui's context menu.

- [ ] **Step 7: Verify build and test**

Run: `cargo build && cargo run`
Test: Toggle grid overlay in settings, verify rendering at various zoom levels. Drag from ruler to create guides. Toggle thirds and safe zones.

- [ ] **Step 8: Commit**

```bash
git add src/ui/preview_panel.rs src/ui/transform_handles.rs
git commit -m "feat: implement grid overlay, custom guides, thirds, and safe zones"
```

---

## Task 7: Rotation

**Files:**
- Modify: `src/ui/transform_handles.rs` (rotation drag mode, rendering)
- Modify: `src/ui/preview_panel.rs` (rotated source rendering considerations)

- [ ] **Step 1: Add DragMode::Rotate variant**

```rust
Rotate {
    start_mouse: Pos2,
    center: Pos2,          // Source center in screen space
    start_angle: f32,      // Angle of initial click relative to center
    start_rotation: f32,   // Source's rotation at drag start
},
```

- [ ] **Step 2: Enter rotation mode on Cmd+corner drag**

In the handle hit-test section, when a corner is hit AND Cmd/Super is held, enter `DragMode::Rotate` instead of `DragMode::Resize`:

```rust
if let Some(handle) = hit_test_handles(mouse_pos, screen_rect) {
    let is_corner = matches!(handle, HandlePosition::TopLeft | HandlePosition::TopRight | HandlePosition::BottomLeft | HandlePosition::BottomRight);
    if is_corner && ui.input(|i| i.modifiers.command) {
        let center = screen_rect.center();
        let angle = (mouse_pos - center).angle();
        drag = DragMode::Rotate {
            start_mouse: mouse_pos,
            center,
            start_angle: angle,
            start_rotation: transform.rotation,
        };
        state.begin_continuous_edit();
    } else {
        // Normal resize
    }
}
```

- [ ] **Step 3: Implement rotation drag handler**

```rust
DragMode::Rotate { center, start_angle, start_rotation, .. } => {
    let current_angle = (current_mouse - *center).angle();
    let delta_angle = (current_angle - start_angle).to_degrees();
    let mut new_rotation = start_rotation + delta_angle;

    // Normalize to 0..360
    new_rotation = new_rotation.rem_euclid(360.0);

    // Shift: snap to 15-degree increments
    if ui.input(|i| i.modifiers.shift) {
        new_rotation = (new_rotation / 15.0).round() * 15.0;
    }

    transform.rotation = new_rotation;
}
```

- [ ] **Step 4: Update hit-testing for rotated sources**

Replace AABB hit-testing with oriented bounding box (OBB) test. When a source has `rotation != 0.0`, the mouse position is inverse-rotated around the source center before testing against the axis-aligned rect:

```rust
fn point_in_rotated_rect(point: Pos2, rect: Rect, rotation_deg: f32) -> bool {
    if rotation_deg == 0.0 {
        return rect.contains(point);
    }
    let center = rect.center();
    let angle = -rotation_deg.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let p = point - center;
    let rotated = pos2(p.x * cos - p.y * sin + center.x, p.x * sin + p.y * cos + center.y);
    rect.contains(rotated)
}
```

- [ ] **Step 5: Draw rotated selection outline and handles**

When drawing the selection outline and handles for a rotated source, apply a rotation transform. egui's `Painter` doesn't have native rotation, so compute rotated corner positions manually:

```rust
fn rotated_corners(rect: Rect, rotation_deg: f32) -> [Pos2; 4] {
    let center = rect.center();
    let angle = rotation_deg.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    corners.map(|c| {
        let p = c - center;
        pos2(p.x * cos - p.y * sin + center.x, p.x * sin + p.y * cos + center.y)
    })
}
```

Draw the outline as 4 line segments between rotated corners. Place handles at the rotated corner/edge positions.

- [ ] **Step 6: Verify build and test**

Run: `cargo build && cargo run`
Test: Select a source, Cmd+drag a corner to rotate. Hold Shift to snap to 15-degree increments. Verify hit-testing works on rotated sources.

- [ ] **Step 7: Commit**

```bash
git add src/ui/transform_handles.rs
git commit -m "feat: implement source rotation via Cmd+corner drag"
```

---

## Task 8: Keyboard Shortcuts

**Files:**
- Modify: `src/main.rs:593-671` (keyboard handler)

- [ ] **Step 1: Add zoom shortcuts (Cmd+0, Cmd+1)**

In the keyboard handler section of `main.rs`, add:

```rust
// Cmd+0: Fit to panel (reset zoom/pan)
if self.modifiers.super_key() && *key_code == KeyCode::Digit0 {
    // Reset zoom/pan — store a signal in AppState that preview_panel reads
    let mut app_state = self.state.lock().unwrap();
    app_state.reset_preview_zoom = true;
    return;
}

// Cmd+1: 100% zoom
if self.modifiers.super_key() && *key_code == KeyCode::Digit1 {
    let mut app_state = self.state.lock().unwrap();
    app_state.set_preview_zoom_100 = true;
    return;
}
```

Add `reset_preview_zoom: bool` and `set_preview_zoom_100: bool` flags to `AppState`. In `preview_panel.rs`, check these flags and apply:

```rust
if state.reset_preview_zoom {
    view.zoom = 1.0;
    view.pan_offset = Vec2::ZERO;
    state.reset_preview_zoom = false;
}
if state.set_preview_zoom_100 {
    // Calculate zoom for 1:1 pixel mapping
    let base = letterboxed_rect(panel_rect, canvas_w, canvas_h);
    view.zoom = canvas_w as f32 / base.width();
    view.pan_offset = Vec2::ZERO;
    state.set_preview_zoom_100 = false;
}
```

- [ ] **Step 2: Add arrow key nudge**

```rust
// Arrow keys: nudge selected sources
if matches!(key_code, KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::ArrowLeft | KeyCode::ArrowRight) {
    let egui_wants_input = /* same check as delete */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        if app_state.selected_source_ids.is_empty() { return; }

        let step = if shift { 10.0 } else { 1.0 };
        let (dx, dy) = match key_code {
            KeyCode::ArrowUp => (0.0, -step),
            KeyCode::ArrowDown => (0.0, step),
            KeyCode::ArrowLeft => (-step, 0.0),
            KeyCode::ArrowRight => (step, 0.0),
            _ => unreachable!(),
        };

        // Batch undo: check if last nudge was within 500ms
        let now = std::time::Instant::now();
        let batch = app_state.last_nudge_time
            .map(|t| now.duration_since(t).as_millis() < 500)
            .unwrap_or(false);
        if !batch {
            // Push undo snapshot for new nudge sequence
            app_state.mark_dirty();
        }
        app_state.last_nudge_time = Some(now);

        // Apply delta to all selected sources
        let scene = app_state.active_scene_mut().unwrap();
        for &id in &app_state.selected_source_ids.clone() {
            if let Some(ss) = scene.find_source_mut(id) {
                let lib = app_state.find_library_source(id);
                let mut t = ss.resolve_transform(lib.unwrap());
                t.x += dx;
                t.y += dy;
                ss.overrides.transform = Some(t);
            }
        }
        app_state.scenes_dirty = true;
        app_state.scenes_last_changed = std::time::Instant::now();
        return;
    }
}
```

Add `last_nudge_time: Option<std::time::Instant>` to `AppState`.

- [ ] **Step 3: Add Cmd+A (select all)**

```rust
if self.modifiers.super_key() && *key_code == KeyCode::KeyA {
    let egui_wants_input = /* check */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        if let Some(scene) = app_state.active_scene() {
            let ids: Vec<SourceId> = scene.sources.iter()
                .filter(|ss| !ss.resolve_locked())
                .map(|ss| ss.source_id)
                .collect();
            app_state.selected_source_ids = ids;
            app_state.primary_selected_id = app_state.selected_source_ids.last().copied();
        }
        return;
    }
}
```

- [ ] **Step 4: Add copy/paste/duplicate (Cmd+C, Cmd+V, Cmd+Shift+V, Cmd+D)**

```rust
// Cmd+C: Copy
if self.modifiers.super_key() && *key_code == KeyCode::KeyC {
    let egui_wants_input = /* check */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        app_state.clipboard.clear();
        let scene = app_state.active_scene().cloned();
        if let Some(scene) = scene {
            for &id in &app_state.selected_source_ids {
                if let Some(ss) = scene.find_source(id) {
                    app_state.clipboard.push(ClipboardEntry {
                        library_source_id: ss.source_id,
                        overrides_snapshot: ss.overrides.clone(),
                    });
                }
            }
        }
        return;
    }
}

// Cmd+V: Paste as reference
if self.modifiers.super_key() && !shift && *key_code == KeyCode::KeyV {
    let egui_wants_input = /* check */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        if app_state.clipboard.is_empty() { return; }
        let entries = app_state.clipboard.clone();
        let mut new_ids = Vec::new();
        for entry in &entries {
            let mut overrides = entry.overrides_snapshot.clone();
            // Offset by +20, +20
            if let Some(ref mut t) = overrides.transform {
                t.x += 20.0;
                t.y += 20.0;
            }
            let ss = SceneSource {
                source_id: entry.library_source_id,
                overrides,
            };
            if let Some(scene) = app_state.active_scene_mut() {
                scene.sources.push(ss);
                new_ids.push(entry.library_source_id);
            }
        }
        app_state.selected_source_ids = new_ids;
        app_state.primary_selected_id = app_state.selected_source_ids.last().copied();
        app_state.mark_dirty();
        return;
    }
}

// Cmd+Shift+V: Paste as clone
if self.modifiers.super_key() && shift && *key_code == KeyCode::KeyV {
    let egui_wants_input = /* check */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        if app_state.clipboard.is_empty() { return; }
        let entries = app_state.clipboard.clone();
        let mut new_ids = Vec::new();
        for entry in &entries {
            if let Some(lib) = app_state.find_library_source(entry.library_source_id).cloned() {
                let new_id = SourceId(app_state.next_source_id);
                app_state.next_source_id += 1;
                let mut new_lib = lib.clone();
                new_lib.id = new_id;
                new_lib.name = format!("{} (Copy)", lib.name);
                app_state.library.push(new_lib);

                let mut overrides = entry.overrides_snapshot.clone();
                if let Some(ref mut t) = overrides.transform {
                    t.x += 20.0;
                    t.y += 20.0;
                }
                let ss = SceneSource { source_id: new_id, overrides };
                if let Some(scene) = app_state.active_scene_mut() {
                    scene.sources.push(ss);
                    new_ids.push(new_id);
                }
            }
        }
        app_state.selected_source_ids = new_ids;
        app_state.primary_selected_id = app_state.selected_source_ids.last().copied();
        app_state.mark_dirty();
        return;
    }
}

// Cmd+D: Duplicate (copy + paste-as-clone in one step)
if self.modifiers.super_key() && *key_code == KeyCode::KeyD {
    // Same as Cmd+C then Cmd+Shift+V, but inline
    // ... (same logic as paste-as-clone but reads from current selection, not clipboard)
}
```

- [ ] **Step 5: Add z-order shortcuts (Cmd+], Cmd+[, Cmd+Shift+], Cmd+Shift+[)**

```rust
if self.modifiers.super_key() && *key_code == KeyCode::BracketRight {
    let mut app_state = self.state.lock().unwrap();
    if shift {
        // Bring to front — move to end of sources list
        // For each selected source, remove and re-add at end
    } else {
        // Bring forward — swap with next in list
        for &id in &app_state.selected_source_ids.clone() {
            if let Some(scene) = app_state.active_scene_mut() {
                scene.move_source_up(id); // Note: "up" in data = forward in z-order
            }
        }
    }
    app_state.mark_dirty();
    return;
}
// Same pattern for BracketLeft (send backward / send to back)
```

- [ ] **Step 6: Add Cmd+L (toggle lock)**

```rust
if self.modifiers.super_key() && *key_code == KeyCode::KeyL {
    let egui_wants_input = /* check */;
    if !egui_wants_input {
        let mut app_state = self.state.lock().unwrap();
        let ids = app_state.selected_source_ids.clone();
        if let Some(scene) = app_state.active_scene_mut() {
            for id in ids {
                if let Some(ss) = scene.find_source_mut(id) {
                    let currently_locked = ss.resolve_locked();
                    ss.overrides.locked = Some(!currently_locked);
                }
            }
        }
        app_state.mark_dirty();
        return;
    }
}
```

- [ ] **Step 7: Update delete handler for multi-select**

Update the existing delete handler to remove all selected sources:

```rust
if matches!(key_code, KeyCode::Delete | KeyCode::Backspace) {
    // ... existing guards ...
    let mut app_state = self.state.lock().unwrap();
    if !app_state.selected_source_ids.is_empty() {
        let ids = app_state.selected_source_ids.clone();
        if let Some(scene_id) = app_state.active_scene_id {
            for id in ids {
                crate::ui::sources_panel::remove_source_from_scene(
                    &mut app_state, &cmd_tx, scene_id, id,
                );
            }
        }
        app_state.deselect_all();
    } else if let Some(src_id) = app_state.selected_library_source_id {
        crate::ui::library_panel::delete_source_cascade(&mut app_state, src_id);
    }
    return;
}
```

- [ ] **Step 8: Verify build and test all shortcuts**

Run: `cargo build && cargo run`
Test each shortcut: Cmd+0/1 (zoom), arrows (nudge), Shift+arrows (nudge 10px), Cmd+A (select all), Cmd+C/V (copy/paste reference), Cmd+Shift+V (paste clone), Cmd+D (duplicate), Cmd+]/[ (z-order), Cmd+L (lock), DEL (multi-delete).

- [ ] **Step 9: Commit**

```bash
git add src/main.rs src/state.rs src/ui/preview_panel.rs
git commit -m "feat: add keyboard shortcuts for nudge, copy/paste, z-order, lock, zoom"
```

---

## Task 9: Source Locking UI

**Files:**
- Modify: `src/ui/sources_panel.rs:428-599` (draw_source_row)
- Modify: `src/ui/transform_handles.rs` (lock dimming, interaction guard)

- [ ] **Step 1: Add lock icon to source row in sources panel**

In `draw_source_row` in `sources_panel.rs`, add a lock/unlock icon button next to the visibility eye icon:

```rust
// After the visibility eye button, add lock toggle
let locked = ss.resolve_locked();
let lock_icon = if locked { phosphor::regular::LOCK } else { phosphor::regular::LOCK_OPEN };
let lock_response = ui.add(egui::Button::new(
    egui::RichText::new(lock_icon).size(14.0).color(if locked { TEXT_MUTED } else { TEXT_MUTED.linear_multiply(0.5) })
).frame(false));
if lock_response.clicked() {
    ss.overrides.locked = Some(!locked);
    state.mark_dirty();
}
```

Check that the `phosphor` icons crate has lock icons. If not, use text labels "🔒"/"🔓" or similar available icons.

- [ ] **Step 2: Guard transform handle interaction for locked sources**

In `draw_transform_handles`, when checking if the user can interact with a source:

```rust
// After resolving the selected source's scene source
let is_locked = ss.resolve_locked();

// Draw handles dimmed if locked
if is_locked {
    // Draw outline and handles with reduced opacity
    let dimmed = TEXT_PRIMARY.linear_multiply(0.3);
    painter.rect_stroke(screen_rect, 0.0, Stroke::new(1.0, dimmed));
    // Skip drag interaction — don't enter any DragMode
} else {
    // Normal handle drawing and interaction
    draw_handles(&painter, screen_rect);
    // ... drag logic ...
}
```

- [ ] **Step 3: Skip locked sources in marquee select**

In the marquee selection logic, skip sources where `resolve_locked()` is true (already specified in Task 4 Step 2, but verify it's implemented).

- [ ] **Step 4: Verify build and test**

Run: `cargo build && cargo run`
Test: Lock a source via icon in sources panel. Verify it can't be dragged. Verify Cmd+L toggles lock. Verify marquee skips locked sources but Shift+click can still select them.

- [ ] **Step 5: Commit**

```bash
git add src/ui/sources_panel.rs src/ui/transform_handles.rs
git commit -m "feat: add source locking with lock icon and interaction guard"
```

---

## Task 10: Settings UI for Grid & Guides

**Files:**
- Modify: `src/ui/settings/` (whichever file handles General settings)

- [ ] **Step 1: Find the settings UI file**

Search for where `snap_to_grid` and `snap_grid_size` are rendered in the settings UI. This is likely in `src/ui/settings/` or wherever the General settings tab is drawn.

- [ ] **Step 2: Add grid/guide controls to settings**

Add UI controls for the new settings fields:

```rust
// Grid section
ui.heading("Grid & Guides");

ui.checkbox(&mut settings.general.show_grid, "Show grid overlay");
ui.checkbox(&mut settings.general.snap_to_grid, "Snap to grid");

// Grid preset dropdown
egui::ComboBox::from_label("Grid preset")
    .selected_text(&settings.general.grid_preset)
    .show_ui(ui, |ui| {
        for preset in ["8", "16", "32", "64", "thirds", "quarters", "custom"] {
            ui.selectable_value(&mut settings.general.grid_preset, preset.to_string(), preset);
        }
    });

if settings.general.grid_preset == "custom" {
    ui.add(egui::Slider::new(&mut settings.general.snap_grid_size, 1.0..=200.0).text("Grid size"));
}

ui.checkbox(&mut settings.general.show_thirds, "Rule of thirds");
ui.checkbox(&mut settings.general.show_safe_zones, "Safe zones (action/title)");
ui.checkbox(&mut settings.general.show_guides, "Show custom guides");

// Color pickers
// Grid color + opacity
// Guide color + opacity
```

- [ ] **Step 3: Verify build and test**

Run: `cargo build && cargo run`
Test: Open settings, toggle grid options, verify changes reflect in preview.

- [ ] **Step 4: Commit**

```bash
git add src/ui/settings/
git commit -m "feat: add grid and guide settings UI"
```

---

## Task 11: Properties Panel — Rotation Field

**Files:**
- Modify: `src/ui/properties_panel.rs`

- [ ] **Step 1: Add rotation field to transform section**

Find where X/Y/W/H are displayed in the properties panel. Add a rotation field after them:

```rust
// After width/height fields
ui.horizontal(|ui| {
    ui.label("Rotation");
    let mut rotation = transform.rotation;
    let response = ui.add(
        egui::DragValue::new(&mut rotation)
            .speed(1.0)
            .suffix("°")
            .range(0.0..=360.0)
    );
    if response.changed() {
        transform.rotation = rotation.rem_euclid(360.0);
        ss.overrides.transform = Some(transform);
        state.mark_dirty();
    }
});
```

- [ ] **Step 2: Verify build and test**

Run: `cargo build && cargo run`
Test: Select a rotated source, verify rotation shows in properties. Edit the value, verify preview updates.

- [ ] **Step 3: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat: add rotation field to properties panel"
```

---

## Task 12: Integration Testing & Polish

**Files:**
- Modify: `src/ui/transform_handles.rs` (tests)

- [ ] **Step 1: Add unit tests for magnetic snapping**

```rust
#[test]
fn magnetic_snap_attracts_within_zone() {
    let targets = vec![100.0];
    // Value within attraction zone (12px) should snap
    assert_eq!(snap_value(108.0, &targets, 12.0), 100.0);
    // Value outside attraction zone should not snap
    assert_eq!(snap_value(113.0, &targets, 12.0), 113.0);
}
```

- [ ] **Step 2: Add unit tests for rotation hit-testing**

```rust
#[test]
fn point_in_rotated_rect_45_degrees() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
    // Center should always be inside
    assert!(point_in_rotated_rect(pos2(50.0, 50.0), rect, 45.0));
    // A point outside the AABB but inside the rotated rect
    // (depends on geometry, test specific known point)
}
```

- [ ] **Step 3: Add unit tests for selection helpers**

```rust
#[test]
fn toggle_source_selection_adds_and_removes() {
    let mut state = /* create test AppState */;
    let id = SourceId(1);
    state.toggle_source_selection(id);
    assert!(state.is_source_selected(id));
    state.toggle_source_selection(id);
    assert!(!state.is_source_selected(id));
}
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 5: Run clippy and format check**

Run: `cargo clippy && cargo fmt --check`
Fix any warnings or formatting issues.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "test: add unit tests for magnetic snap, rotation, and selection"
```
