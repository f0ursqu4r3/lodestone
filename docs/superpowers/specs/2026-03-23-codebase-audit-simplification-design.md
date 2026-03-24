# Codebase Audit & Simplification

**Date**: 2026-03-23
**Approach**: Bottom-up — shared utilities first, then decompose functions, then split files, then clean dead code.

## Problem

The codebase has grown organically through feature addition. Seven functions exceed 300 lines, `source_icon()` is duplicated in 2 files, styled rect patterns are copy-pasted across all panels, and five files exceed 500 lines mixing unrelated concerns. This creates cognitive overhead that slows feature work and increases the risk of divergent duplicates.

## Constraints

- Pure structural refactoring — no behavioral changes.
- Each commit compiles and passes `cargo clippy`.
- KISS: no new abstractions beyond what eliminates concrete duplication.
- GPL-3.0 license, Rust conventions per CLAUDE.md.

---

## Phase 1: Shared Utilities

### New file: `src/ui/draw_helpers.rs` (~150 lines)

**Moved from existing files:**
- `source_icon(source_type: &SourceType) -> &'static str` — from `library_panel.rs` and `sources_panel.rs` (duplicated)
- `draw_segmented_buttons(ui, id_salt, buttons) -> Option<usize>` — from `library_panel.rs` (general widget)
- `with_opacity(color: Color32, alpha: f32) -> Color32` — from `sources_panel.rs` (general color utility)

**New helpers consolidating repeated inline patterns:**
- `draw_selection_highlight(painter, rect, color)` — `rect_filled` + `RADIUS_SM` pattern used across all panels
- `draw_styled_rect(painter, rect, fill, border)` — fill + optional stroke combo repeated everywhere

Add `pub mod draw_helpers;` to `ui/mod.rs`. Update all imports in consuming files.

---

## Phase 2: Panel Function Decomposition

Break mega `draw()` functions into focused helpers within each file. Target: no function exceeds ~150 lines.

### `library_panel.rs` (~930 lines → ~6 functions)

- `draw()` — orchestrator (~50 lines): load settings, draw header, dispatch to view
- `draw_header()` — add button + segmented toggles
- `draw_by_type_view()` / `draw_folders_view()` — already exist, stay as-is
- `draw_source_row()` — already exists, stays
- `draw_source_grid()` — already exists, stays
- Remove local `draw_segmented_buttons()` and `source_icon()` (moved to `draw_helpers`)

### `sources_panel.rs` (~660 lines → ~5 functions)

- `draw()` — orchestrator: header, source list, add popup
- `draw_source_row()` — single source row with visibility toggle, drag handle
- `draw_add_from_library_popup()` — popup menu for adding library sources to scene
- `draw_context_menu()` — right-click menu logic
- Remove local `source_icon()` and `with_opacity()` (moved to `draw_helpers`)

### `properties_panel.rs` (556 lines → ~4 functions)

- `draw()` — orchestrator: source header, dispatch to type-specific editor
- `draw_transform_section()` — x/y/w/h/opacity controls
- `draw_source_properties()` — match on source type, call type-specific UI
- `draw_override_indicators()` — per-scene override badges

### `scenes_panel.rs` (461 lines → ~4 functions)

- `draw()` — orchestrator: header, scene grid
- `draw_scene_card()` — single scene thumbnail + label
- `draw_inline_rename()` — rename text field logic
- `draw_context_menu()` — right-click menu

---

## Phase 3: File Splits

### `layout/render.rs` (1507 lines → 4 files)

- `layout/render.rs` — entry point: `render_dockview()`, menu bar, top-level dispatch (~200 lines)
- `layout/render_grid.rs` — `render_dividers()`, grid group rendering, resize interaction (~300 lines)
- `layout/render_tabs.rs` — `render_tab_bar()`, `render_content()`, tab close button, text truncation (~400 lines)
- `layout/render_floating.rs` — `render_floating_chrome()`, floating title bar, floating group render (~150 lines)

### `layout/tree.rs` (1064 lines → 3 files)

- `layout/tree.rs` — data structures only: `PanelType`, `PanelId`, `GroupId`, `Group`, `SplitNode`, `DockLayout` structs (~200 lines)
- `layout/tree_builders.rs` — construction and mutation: `new()`, `default_layout()`, `split_group()`, `take_tab()`, `insert_floating_into_grid()` (~400 lines)
- `layout/tree_queries.rs` — read-only traversal: `find_parent()`, `find_node_for_group()`, `collect_groups_with_rects()`, `collect_all_panels()` (~200 lines)

### `settings_window.rs` (978 lines → module directory)

- `settings/mod.rs` — window chrome, sidebar navigation, section dispatch (~200 lines)
- `settings/video.rs` — resolution, codec, bitrate, FPS (~150 lines)
- `settings/audio.rs` — mic/system device, levels, encoder (~150 lines)
- `settings/stream.rs` — RTMP, platform, stream key (~100 lines)
- `settings/hotkeys.rs` — hotkey binding UI (~100 lines)
- `settings/appearance.rs` — theme, accent color, font size (~100 lines)

### `window.rs` (620 lines → 2 files)

- `window.rs` — `WindowState` struct, surface setup, resize, core render loop (~250 lines)
- `window_actions.rs` — `apply_layout_action()` match dispatch (~350 lines)

---

## Phase 4: Dead Code & GStreamer Cleanup

### Dead code audit

- **Remove**: `gstreamer/error.rs` (whole module marked dead), unused variants in `gstreamer/commands.rs`, unused `RgbaFrame` field in `gstreamer/types.rs`
- **Keep**: Scene override resolvers (`resolve_muted()`, `resolve_volume()`, etc. in `scene.rs`) — coherent API for upcoming audio mixing. Remove `#[allow(dead_code)]`, make `pub`.

### `gstreamer/thread.rs` — extract command handlers

Extract each `GstCommand` match arm into a named method:
- `handle_add_capture()`, `handle_remove_capture()`, `handle_start_stream()`, `handle_start_record()`, etc.
- Each handler ≤50 lines. No new files.

### Extract compositor shader

- Move inline WGSL string from `renderer/compositor.rs` to `renderer/shaders/compositor.wgsl`
- Load with `include_str!("shaders/compositor.wgsl")`

---

## Commit Sequence

1. Extract `draw_helpers.rs` — shared utilities, update imports
2. Decompose `library_panel.rs` draw function
3. Decompose `sources_panel.rs` draw function
4. Decompose `properties_panel.rs` draw function
5. Decompose `scenes_panel.rs` draw function
6. Split `layout/render.rs` → 4 files
7. Split `layout/tree.rs` → 3 files
8. Split `settings_window.rs` → settings module
9. Split `window.rs` → extract `window_actions.rs`
10. GStreamer: extract command handlers, audit dead code
11. Dead code pass across remaining files
12. Extract compositor shader to `.wgsl` file

Each commit compiles and passes `cargo clippy`. No behavioral changes.
