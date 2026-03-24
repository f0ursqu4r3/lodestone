# Codebase Audit & Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the Lodestone codebase bottom-up — extract shared utilities, decompose mega-functions, split oversized files, and clean dead code — with zero behavioral changes.

**Architecture:** Bottom-up approach. Phase 1 extracts shared drawing helpers. Phase 2 decomposes panel `draw()` functions into focused helpers. Phase 3 splits files exceeding 500 lines into focused modules. Phase 4 cleans dead code and extracts the compositor shader.

**Tech Stack:** Rust, egui, wgpu, GStreamer, winit

---

## File Structure

### New files created:
- `src/ui/draw_helpers.rs` — shared drawing utilities (source_icon, segmented buttons, styled rects)
- `src/ui/layout/render_grid.rs` — grid/divider rendering extracted from render.rs
- `src/ui/layout/render_tabs.rs` — tab bar and content rendering extracted from render.rs
- `src/ui/layout/render_floating.rs` — floating window chrome extracted from render.rs
- `src/ui/layout/tree_builders.rs` — DockLayout construction/mutation methods
- `src/ui/layout/tree_queries.rs` — DockLayout read-only traversal methods
- `src/ui/settings/mod.rs` — settings window chrome, sidebar, dispatch (replaces settings_window.rs)
- `src/ui/settings/general.rs` — general settings section
- `src/ui/settings/stream.rs` — stream/output settings section
- `src/ui/settings/audio.rs` — audio settings section
- `src/ui/settings/video.rs` — video settings section
- `src/ui/settings/hotkeys.rs` — hotkeys settings section
- `src/ui/settings/appearance.rs` — appearance settings section
- `src/ui/settings/advanced.rs` — advanced settings section
- `src/window_actions.rs` — LayoutAction dispatch extracted from window.rs
- `src/renderer/shaders/compositor.wgsl` — compositor shader extracted from Rust string

### Files modified:
- `src/ui/mod.rs` — add draw_helpers module, replace settings_window with settings
- `src/ui/library_panel.rs` — remove source_icon/draw_segmented_buttons, decompose draw()
- `src/ui/sources_panel.rs` — remove source_icon/with_opacity, decompose draw()
- `src/ui/properties_panel.rs` — decompose draw()
- `src/ui/scenes_panel.rs` — decompose draw()
- `src/ui/layout/mod.rs` — add render_grid, render_tabs, render_floating, tree_builders, tree_queries modules
- `src/ui/layout/render.rs` — extract functions to new files, keep orchestrator
- `src/ui/layout/tree.rs` — keep data structures, move methods to builders/queries
- `src/window.rs` — extract action dispatch to window_actions.rs
- `src/gstreamer/thread.rs` — extract match arms to named handler methods
- `src/gstreamer/mod.rs` — remove error module re-export
- `src/scene.rs` — remove #[allow(dead_code)] from override resolvers, make pub
- `src/renderer/compositor.rs` — replace inline shader with include_str!()
- `src/renderer/mod.rs` — clean dead code marker if appropriate

### Files deleted:
- `src/ui/settings_window.rs` — replaced by settings/ module

---

## Task 1: Extract `draw_helpers.rs`

**Files:**
- Create: `src/ui/draw_helpers.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/library_panel.rs`
- Modify: `src/ui/sources_panel.rs`

- [ ] **Step 1: Create `src/ui/draw_helpers.rs`**

Create the file with the following functions moved from their current locations:

1. `source_icon()` from `library_panel.rs:32-41` (identical copy in `sources_panel.rs:24-33`)
2. `draw_segmented_buttons()` from `library_panel.rs:45-112`
3. `with_opacity()` from `sources_panel.rs:656-663`

Plus two new helpers consolidating repeated inline patterns:

```rust
use crate::scene::SourceType;
use crate::ui::theme::{BG_ELEVATED, BORDER, DEFAULT_ACCENT, RADIUS_SM, TEXT_MUTED, TEXT_PRIMARY, accent_dim};
use egui::{Color32, CornerRadius, Rect, Sense, vec2};

/// Return a Phosphor icon for a given source type.
pub fn source_icon(source_type: &SourceType) -> &'static str {
    match source_type {
        SourceType::Display => egui_phosphor::regular::MONITOR,
        SourceType::Camera => egui_phosphor::regular::VIDEO_CAMERA,
        SourceType::Image => egui_phosphor::regular::IMAGE,
        SourceType::Browser => egui_phosphor::regular::BROWSER,
        SourceType::Audio => egui_phosphor::regular::SPEAKER_HIGH,
        SourceType::Window => egui_phosphor::regular::APP_WINDOW,
    }
}

/// Draw a segmented button group: connected icon toggles with a shared background.
/// Returns `Some(index)` if a button was clicked.
pub fn draw_segmented_buttons(
    ui: &mut egui::Ui,
    id_salt: &str,
    buttons: &[(&str, &str, bool)], // (icon, tooltip, is_active)
) -> Option<usize> {
    // ... (move full body from library_panel.rs:45-112)
}

/// Multiply all RGBA channels of a color by an opacity factor.
pub fn with_opacity(color: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * opacity) as u8,
        (color.g() as f32 * opacity) as u8,
        (color.b() as f32 * opacity) as u8,
        (color.a() as f32 * opacity) as u8,
    )
}

/// Draw a filled rect with rounded corners — the standard selection/hover highlight.
pub fn draw_selection_highlight(painter: &egui::Painter, rect: Rect, color: Color32) {
    painter.rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), color);
}

/// Draw a filled rect with optional border stroke.
pub fn draw_styled_rect(
    painter: &egui::Painter,
    rect: Rect,
    fill: Color32,
    border: Option<(f32, Color32)>,
) {
    painter.rect_filled(rect, CornerRadius::same(RADIUS_SM as u8), fill);
    if let Some((width, color)) = border {
        painter.rect_stroke(
            rect,
            CornerRadius::same(RADIUS_SM as u8),
            egui::Stroke::new(width, color),
            egui::StrokeKind::Inside,
        );
    }
}
```

- [ ] **Step 2: Register the module in `src/ui/mod.rs`**

Add `pub mod draw_helpers;` after the existing `pub mod theme;` line.

- [ ] **Step 3: Update `library_panel.rs`**

- Delete `source_icon()` (lines 32-41)
- Delete `draw_segmented_buttons()` (lines 45-112)
- Add `use crate::ui::draw_helpers::{source_icon, draw_segmented_buttons};` to imports
- Replace any inline `rect_filled` selection highlights with `draw_selection_highlight()` calls where the pattern matches exactly

- [ ] **Step 4: Update `sources_panel.rs`**

- Delete `source_icon()` (lines 24-33)
- Delete `with_opacity()` (lines 656-663)
- Add `use crate::ui::draw_helpers::{source_icon, with_opacity};` to imports
- Replace any inline selection highlights with `draw_selection_highlight()` calls where appropriate

- [ ] **Step 5: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build, no warnings from changed files.

- [ ] **Step 6: Commit**

```bash
git add src/ui/draw_helpers.rs src/ui/mod.rs src/ui/library_panel.rs src/ui/sources_panel.rs
git commit -m "refactor: extract shared draw_helpers (source_icon, segmented buttons, with_opacity)"
```

---

## Task 2: Decompose `library_panel.rs` draw()

**Files:**
- Modify: `src/ui/library_panel.rs`

The `draw()` function at line 126 is ~164 lines of orchestration. It's already reasonably structured with existing helpers (`draw_by_type_view`, `draw_folders_view`, etc.), but the header rendering and settings persistence are inline. Extract a `draw_header()` function.

- [ ] **Step 1: Extract `draw_header()`**

Move lines 139-196 (the `ui.horizontal` block containing add button + segmented toggles) into a new function:

```rust
/// Draw the library panel header: add button + view/display segmented toggles.
/// Returns updated (LibraryView, LibraryDisplayMode).
fn draw_header(
    ui: &mut egui::Ui,
    state: &mut AppState,
    view: LibraryView,
    display_mode: LibraryDisplayMode,
) -> (LibraryView, LibraryDisplayMode) {
    let mut view = view;
    let mut display_mode = display_mode;
    ui.horizontal(|ui| {
        draw_add_button(ui, state);
        // ... rest of header from lines 142-195
    });
    (view, display_mode)
}
```

Update `draw()` to call `let (view, display_mode) = draw_header(ui, state, view, display_mode);`.

- [ ] **Step 2: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 3: Commit**

```bash
git add src/ui/library_panel.rs
git commit -m "refactor: extract draw_header() from library_panel draw()"
```

---

## Task 3: Decompose `sources_panel.rs` draw()

**Files:**
- Modify: `src/ui/sources_panel.rs`

The `draw()` function at line 36 is ~520 lines. Extract the "add from library" popup and the per-row rendering.

- [ ] **Step 1: Extract `draw_add_from_library_popup()`**

Move lines 73-152 (the popup showing available library sources to add to the current scene) into a standalone function:

```rust
/// Draw the "add source from library" popup.
/// Returns Some(source_id) if a source was selected.
fn draw_add_from_library_popup(
    ui: &mut egui::Ui,
    state: &mut AppState,
    scene_id: SceneId,
    popup_id: egui::Id,
) -> Option<SourceId> {
    // ... popup body
}
```

- [ ] **Step 2: Extract `draw_source_row()`**

The per-source rendering logic (lines ~292-456: paint rect, selection, icon, name, eye icon, context menu, separator) is already partly structured. Extract it into a function if not already one:

```rust
/// Draw a single source row in the scene source list.
fn draw_source_row(
    ui: &mut egui::Ui,
    state: &mut AppState,
    source_id: SourceId,
    index: usize,
    is_selected: bool,
    offset_y: f32,
) -> SourceRowAction {
    // ... per-row rendering
}
```

Where `SourceRowAction` captures deferred actions (select, start drag, toggle visibility, etc.).

Note: Context menu logic is inlined within `draw_source_row()` rather than extracted as a separate function — it's tightly coupled to the row's state.

- [ ] **Step 3: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 4: Commit**

```bash
git add src/ui/sources_panel.rs
git commit -m "refactor: extract draw_add_from_library_popup and draw_source_row from sources_panel"
```

---

## Task 4: Decompose `properties_panel.rs` draw()

**Files:**
- Modify: `src/ui/properties_panel.rs`

The `draw()` function at line 15 is ~467 lines. Extract the three major sections.

- [ ] **Step 1: Extract `draw_transform_section()`**

Move lines 76-158 (transform editing with override dots in scene mode, direct edit in library mode) into:

```rust
/// Draw the transform (position + size) editing section.
/// Returns true if any value changed.
fn draw_transform_section(
    ui: &mut egui::Ui,
    source: &mut SceneSource,       // if in scene
    lib_source: &mut LibrarySource, // always
    in_scene: bool,
) -> bool {
    // ...
}
```

- [ ] **Step 2: Extract `draw_opacity_section()`**

Move lines 162-236 (opacity slider with override dot) into a similar function.

Note: Override dot/indicator rendering (`override_dot()` at line 485) is called within the transform and opacity sections, so it stays as a shared helper rather than a separate extraction.

- [ ] **Step 3: Extract `draw_source_properties()`**

Move lines 240-474 (the match on SourceProperties dispatching to Display/Image/Window/Camera UI) into:

```rust
/// Draw source-type-specific property controls.
/// Returns true if any value changed.
fn draw_source_properties(
    ui: &mut egui::Ui,
    state: &mut AppState,
    source: &mut LibrarySource,
) -> bool {
    // ...
}
```

- [ ] **Step 4: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 5: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "refactor: extract transform, opacity, and source property sections from properties_panel"
```

---

## Task 5: Decompose `scenes_panel.rs` draw()

**Files:**
- Modify: `src/ui/scenes_panel.rs`

The `draw()` function at line 15 is ~263 lines. The scene card rendering is deeply nested in a row/col grid loop.

- [ ] **Step 1: Extract `draw_scene_card()`**

Move the per-scene rendering (lines ~72-222: border, thumbnail placeholder, name label, selection, context menu) into:

```rust
/// Draw a single scene card in the grid.
/// Returns an optional deferred action (switch scene, delete, rename, etc.).
fn draw_scene_card(
    ui: &mut egui::Ui,
    state: &mut AppState,
    scene: &SceneSnapshot,
    cell_rect: Rect,
    is_active: bool,
) -> Option<SceneAction> {
    // ...
}
```

Note: Inline rename and context menu logic are incorporated into `draw_scene_card()` rather than extracted as separate functions — they're tightly coupled to the card's interaction state.

- [ ] **Step 2: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 3: Commit**

```bash
git add src/ui/scenes_panel.rs
git commit -m "refactor: extract draw_scene_card from scenes_panel"
```

---

## Task 6: Split `layout/render.rs`

**Files:**
- Modify: `src/ui/layout/render.rs` — keep orchestrator + menu bar + drag overlay
- Create: `src/ui/layout/render_grid.rs` — divider rendering
- Create: `src/ui/layout/render_tabs.rs` — tab bar + content rendering
- Create: `src/ui/layout/render_floating.rs` — floating window chrome
- Modify: `src/ui/layout/mod.rs` — add new modules

Current layout of render.rs (1507 lines):
- Lines 15-21: constants (DROP_ZONE_TINT)
- Lines 28-82: LayoutAction enum
- Lines 85-91: paint_grip_dots()
- Lines 93-102: DOCKABLE_TYPES
- Lines 109-114: render_menu_bar()
- Lines 120-286: render_layout() — main orchestrator
- Lines 293-363: render_dividers() → **move to render_grid.rs**
- Lines 370-447: render_drag_overlay()
- Lines 453-827: render_tab_bar() → **move to render_tabs.rs**
- Lines 834-870: render_content() → **move to render_tabs.rs**
- Lines 878-1507: render_floating_chrome() → **move to render_floating.rs**

- [ ] **Step 1: Create `render_grid.rs`**

Move `render_dividers()` (lines 293-363) and any divider-specific helpers. The function needs access to `LayoutAction`, `DockLayout`, `SplitNode`, constants from render.rs. Use `pub(super)` visibility and import from the parent module.

- [ ] **Step 2: Create `render_tabs.rs`**

Move `render_tab_bar()` (lines 453-827) and `render_content()` (lines 834-870). These need `LayoutAction`, `paint_grip_dots`, `DOCKABLE_TYPES`, `DROP_ZONE_TINT`, and dockview types. Import from parent.

- [ ] **Step 3: Create `render_floating.rs`**

Move `render_floating_chrome()` (lines 878-1507). This needs `LayoutAction`, floating group types, theme constants.

- [ ] **Step 4: Update `render.rs`**

- Keep: constants, `LayoutAction` enum, `paint_grip_dots()`, `DOCKABLE_TYPES`, `render_menu_bar()`, `render_layout()`, `render_drag_overlay()`
- Make extracted items `pub(crate)` as needed for the new files
- Replace moved function bodies with calls to the new modules

- [ ] **Step 5: Update `layout/mod.rs`**

Add module declarations:
```rust
pub mod render_grid;
pub mod render_tabs;
pub mod render_floating;
```

- [ ] **Step 6: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 7: Commit**

```bash
git add src/ui/layout/
git commit -m "refactor: split layout/render.rs into render_grid, render_tabs, render_floating"
```

---

## Task 7: Split `layout/tree.rs`

**Files:**
- Modify: `src/ui/layout/tree.rs` — keep data structures only
- Create: `src/ui/layout/tree_builders.rs` — construction + mutation
- Create: `src/ui/layout/tree_queries.rs` — read-only traversal
- Modify: `src/ui/layout/mod.rs` — add new modules

Current tree.rs structure (1064 lines):
- Lines 13-128: PanelType, PanelId, SplitDirection, GroupId, TabEntry (data types)
- Lines 134-210: Group struct + impl
- Lines 217-275: NodeId, SplitNode, FloatingGroup, DropZone, DragState
- Lines 281-294: DockLayout struct
- Lines 296-795+: `impl DockLayout` — mixed builders + queries

- [ ] **Step 1: Identify builder vs query methods**

**Builders** (mutation, construction — move to `tree_builders.rs`):
- `new_single()`, `new_with_ids()`, `default_layout()`
- `split_group()`, `split_group_with_tab()`
- `remove_group_from_grid()`, `insert_at_root()`, `insert_floating_into_grid()`
- `resize()` and any other `&mut self` methods that modify structure

**Queries** (read-only — move to `tree_queries.rs`):
- `find_parent()`, `find_node_for_group()`
- `collect_groups_with_rects()`, `collect_all_panels()`
- Any `&self` methods that traverse/inspect

**Keep in tree.rs**:
- All struct/enum definitions
- `alloc_node_id()`, `root_id()`, `node()`, `nodes()`, `from_parts()` (basic accessors)

- [ ] **Step 2: Create `tree_builders.rs`**

Move builder methods into a separate `impl DockLayout` block. The file imports `DockLayout` and all needed types from `super::tree`.

```rust
use super::tree::*;

impl DockLayout {
    pub fn new_single(panel: PanelType) -> Self { ... }
    pub fn default_layout() -> Self { ... }
    pub fn split_group(...) { ... }
    // etc.
}
```

- [ ] **Step 3: Create `tree_queries.rs`**

Move query methods similarly.

- [ ] **Step 4: Update `tree.rs`**

Remove moved method bodies. Keep all type definitions and basic accessors.

- [ ] **Step 5: Update `layout/mod.rs`**

Add:
```rust
mod tree_builders;
mod tree_queries;
```

(Private modules — their `impl DockLayout` blocks extend the type without needing to be public.)

- [ ] **Step 6: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 7: Commit**

```bash
git add src/ui/layout/
git commit -m "refactor: split layout/tree.rs into tree_builders and tree_queries"
```

---

## Task 8: Split `settings_window.rs` into module

**Files:**
- Delete: `src/ui/settings_window.rs`
- Create: `src/ui/settings/mod.rs`
- Create: `src/ui/settings/general.rs`
- Create: `src/ui/settings/stream.rs`
- Create: `src/ui/settings/audio.rs`
- Create: `src/ui/settings/video.rs`
- Create: `src/ui/settings/hotkeys.rs`
- Create: `src/ui/settings/appearance.rs`
- Create: `src/ui/settings/advanced.rs`
- Modify: `src/ui/mod.rs`

Current settings_window.rs sections:
- Lines 18-73: SettingsCategory enum, SidebarGroup, SIDEBAR_GROUPS
- Lines 75-122: render_native() — public entry
- Lines 124-192: render_sidebar()
- Lines 194-272: render_content_direct() + helpers (section_header, labeled_row, etc.)
- Lines 274-337: draw_general()
- Lines 339-494: draw_stream()
- Lines 496-627: draw_audio()
- Lines 628-725: draw_video()
- Lines 726-776: draw_hotkeys()
- Lines 777-871: draw_appearance()
- Lines 872-916: draw_advanced()
- Lines 917-978: toggle helpers (draw_toggle, toggle_switch)

- [ ] **Step 1: Create `src/ui/settings/mod.rs`**

Move the following from settings_window.rs:
- SettingsCategory enum + impl (lines 18-43)
- SidebarGroup + SIDEBAR_GROUPS (lines 45-73)
- render_native() (lines 75-122)
- render_sidebar() (lines 124-192)
- render_content_direct() (lines 194-227)
- Helper functions: section_header, labeled_row, labeled_row_unimplemented, draw_toggle_unimplemented (lines 229-272)
- Toggle helpers: draw_toggle, toggle_switch (lines 917-978)

Make section helpers `pub(crate)` so the sub-modules can use them.

Declare sub-modules:
```rust
mod general;
mod stream;
mod audio;
mod video;
mod hotkeys;
mod appearance;
mod advanced;
```

Update `render_content_direct()` to call `general::draw()`, `stream::draw()`, etc.

- [ ] **Step 2: Create each section file**

Move each `draw_*()` function to its own file:
- `general.rs` ← `draw_general()` (lines 274-337)
- `stream.rs` ← `draw_stream()` (lines 339-494)
- `audio.rs` ← `draw_audio()` (lines 496-627)
- `video.rs` ← `draw_video()` (lines 628-725)
- `hotkeys.rs` ← `draw_hotkeys()` (lines 726-776)
- `appearance.rs` ← `draw_appearance()` (lines 777-871)
- `advanced.rs` ← `draw_advanced()` (lines 872-916)

Each file exports a single `pub(super) fn draw(ui, state/settings) -> bool`.

- [ ] **Step 3: Update `src/ui/mod.rs`**

Replace `pub mod settings_window;` with `pub mod settings;`.

Update any references to `settings_window::render_native` to `settings::render_native`.

- [ ] **Step 4: Update all callers**

Search for `settings_window::` across the codebase (likely `window.rs` and `main.rs`) and update to `settings::`.

- [ ] **Step 5: Delete `src/ui/settings_window.rs`**

- [ ] **Step 6: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 7: Commit**

```bash
git add src/ui/settings/ src/ui/mod.rs src/window.rs src/main.rs
git rm src/ui/settings_window.rs
git commit -m "refactor: split settings_window.rs into settings/ module with per-section files"
```

---

## Task 9: Split `window.rs` — extract action dispatch

**Files:**
- Modify: `src/window.rs`
- Create: `src/window_actions.rs`

The `render()` method (lines 110-493) has a 250-line match statement (lines 144-391) dispatching `LayoutAction` variants. Extract it.

- [ ] **Step 1: Create `src/window_actions.rs`**

Move the LayoutAction match dispatch into a standalone function:

```rust
use crate::ui::layout::{DockLayout, ...};
use crate::ui::layout::render::LayoutAction;

/// Apply a layout action to the dock layout and manage desktop/floating windows.
pub(crate) fn apply_layout_action(
    action: LayoutAction,
    layout: &mut DockLayout,
    // ... other needed params (desktop_windows, event_loop, etc.)
) {
    match action {
        LayoutAction::Resize { ... } => { ... }
        LayoutAction::SetActiveTab { ... } => { ... }
        // ... all 17 arms from window.rs:144-391
    }
}
```

The exact signature depends on what state the match arms access. Identify all accessed fields from `WindowState` and pass them as parameters (or pass `&mut WindowState` if cleaner).

- [ ] **Step 2: Update `window.rs`**

Replace the inline match block with a call to `window_actions::apply_layout_action(...)`.

Add `mod window_actions;` to `src/main.rs` or make it a sibling module.

- [ ] **Step 3: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 4: Commit**

```bash
git add src/window.rs src/window_actions.rs src/main.rs
git commit -m "refactor: extract LayoutAction dispatch from window.rs into window_actions.rs"
```

---

## Task 10: GStreamer handler extraction

**Files:**
- Modify: `src/gstreamer/thread.rs`

The `handle_command()` method (lines 300-415) has 13 match arms. Extract each into a named method.

- [ ] **Step 1: Extract handler methods**

For each match arm, create a method on `GstThread`:

```rust
impl GstThread {
    fn handle_start_stream(&mut self, config: StreamConfig) { ... }     // line 302
    fn handle_stop_stream(&mut self) { ... }                            // line 327
    fn handle_stop_recording(&mut self) { ... }                         // line 330
    fn handle_start_recording(&mut self, path: PathBuf, format: RecordingFormat) { ... } // line 333
    fn handle_update_encoder(&mut self, config: AudioEncoderConfig) { ... } // line 358
    fn handle_set_audio_device(&mut self, source: String, device_uid: String) { ... } // line 361
    fn handle_set_audio_volume(&mut self, source: String, volume: f64) { ... } // line 365
    fn handle_set_audio_muted(&mut self, source: String, muted: bool) { ... } // line 376
    fn handle_stop_capture(&mut self) { ... }                           // line 387
    fn handle_load_image_frame(&mut self, source_id: SourceId, frame: RgbaFrame) { ... } // line 403
    fn handle_shutdown(&mut self) { ... }                               // line 410
}
```

- [ ] **Step 2: Simplify `handle_command()`**

Replace the match body with single-line dispatches:

```rust
fn handle_command(&mut self, cmd: GstCommand) {
    match cmd {
        GstCommand::StartStream(config) => self.handle_start_stream(config),
        GstCommand::StopStream => self.handle_stop_stream(),
        // ... etc.
    }
}
```

- [ ] **Step 3: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "refactor: extract GstCommand handler methods in gstreamer thread"
```

---

## Task 11: Dead code pass

**Files:**
- Modify: `src/gstreamer/error.rs`
- Modify: `src/gstreamer/mod.rs`
- Modify: `src/gstreamer/commands.rs`
- Modify: `src/gstreamer/types.rs`
- Modify: `src/scene.rs`
- Modify: `src/renderer/mod.rs`

- [ ] **Step 1: Clean `gstreamer/error.rs`**

The module is marked `#[allow(dead_code)]` but `GstError` is actively used in `thread.rs` (error channel sends), `state.rs` (`active_errors: Vec<GstError>`), and re-exported from `gstreamer/mod.rs`. **Keep the file.** Remove the `#[allow(dead_code)]` annotations from the enum and its variants.

- [ ] **Step 2: Clean `gstreamer/commands.rs`**

Remove `#[allow(dead_code)]` from command variants and types that ARE used. For variants truly unused, either remove them or document them as future API.

- [ ] **Step 3: Clean `gstreamer/types.rs`**

Remove `#[allow(dead_code)]` from fields/structs that ARE used. Remove truly unused fields.

- [ ] **Step 4: Clean `scene.rs` override resolvers**

Lines 140-195: Remove `#[allow(dead_code)]` from:
- `resolve_muted()`
- `resolve_volume()`
- `is_visible_overridden()`
- `is_muted_overridden()`
- `is_volume_overridden()`
- `move_source_up()`
- `move_source_down()`

Make them all `pub` — they're a coherent API for audio mixing work.

- [ ] **Step 5: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build. Some methods may trigger "unused" warnings — that's expected for future API. Add targeted `#[allow(unused)]` only if clippy complains AND the method is intentionally kept for future use.

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/ src/scene.rs src/renderer/mod.rs
git commit -m "refactor: clean dead code markers, remove truly unused code"
```

---

## Task 12: Extract compositor shader

**Files:**
- Create: `src/renderer/shaders/compositor.wgsl`
- Modify: `src/renderer/compositor.rs`

- [ ] **Step 1: Create shader file**

Extract the WGSL string from `compositor.rs` lines 39-82 (the content between `r#"` and `"#`) into `src/renderer/shaders/compositor.wgsl`. This is pure WGSL — no Rust string delimiters.

- [ ] **Step 2: Update `compositor.rs`**

Replace:
```rust
const COMPOSITOR_SHADER: &str = r#"
    ...shader code...
"#;
```

With:
```rust
const COMPOSITOR_SHADER: &str = include_str!("shaders/compositor.wgsl");
```

- [ ] **Step 3: Build and lint**

Run: `cargo build && cargo clippy`
Expected: Clean build.

- [ ] **Step 4: Commit**

```bash
git add src/renderer/shaders/compositor.wgsl src/renderer/compositor.rs
git commit -m "refactor: extract compositor WGSL shader to separate file"
```

---

## Verification

After all 12 tasks:

- [ ] **Full build**: `cargo build`
- [ ] **Clippy clean**: `cargo clippy`
- [ ] **Tests pass**: `cargo test`
- [ ] **Format check**: `cargo fmt --check`
- [ ] **No behavioral changes**: Run the app and verify scenes, sources, layout, settings all work identically
