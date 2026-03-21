# Unify Panel Logic

## Problem

The panel group rendering logic (tab bar, "+", grip, content) is duplicated between docked grid groups and floating groups. The floating codepath (~240 lines) is an inline copy inside an `egui::Window` closure that diverges from the shared `render_tab_bar()` / `render_content()` functions used by docked groups. This makes behavior inconsistent and maintenance costly.

## Goal

A single rendering path for panel groups regardless of context (docked in main window, floating in main window, inside detached OS window). The floating container chrome (collapse, title, close) is a thin wrapper around the shared panel group renderer.

## Design

### Component hierarchy

```
Panel Group (shared everywhere)
  ├── Panel Group Header
  │     ├── Panel Tabs (left-aligned)
  │     │     ├── Panel Tab (title + close-on-hover)
  │     │     └── Add Panel Button (+)
  │     └── Panel Grip (right-aligned, drag handle)
  └── Active Panel Content

Floating Container (wrapper, main window only)
  ├── Floating Chrome Header
  │     ├── Collapse Button (left) — docks to grid
  │     ├── Window Title (center) — active panel name
  │     └── Close Button (right) — closes group
  ├── Panel Group  ← shared
  └── Resize handles (edges/corners)
```

### Rendering approach

Replace `egui::Window` for floating panels with a custom `egui::Area`-based container. The rendering becomes:

1. **Docked groups** call `render_tab_bar()` + `render_content()` directly (unchanged).
2. **Floating groups** call a new `render_floating_chrome()` which:
   - Renders the floating chrome header (collapse, title, close)
   - Handles title bar drag (move)
   - Handles edge/corner resize
   - Paints shadow and border
   - Stores the floating window rect for drop target hit testing (currently lines 426-430)
   - Calls `render_tab_bar()` + `render_content()` inside the container

### Interaction consistency

All panel groups use the same `render_tab_bar()` with `TabBarContext` flags controlling context menu items:

| Context | Menu items |
|---------|-----------|
| Docked in main (`is_main=true, is_floating=false`) | Detach, Pop Out to Window, Close Others, Close |
| Floating in main (`is_main=true, is_floating=true`) | Dock to Grid, Pop Out to Window, Close Others, Close |
| Detached OS window (`is_main=false`) | Reattach to Main Window, Close Others, Close |

The grip context menu follows the same pattern. Tab click, drag-to-start, close button, and "+" popup are identical everywhere.

**Behavioral additions from unification:** Floating tabs currently lack drag-to-start support. Using the shared `render_tab_bar()` adds this capability as a side effect — floating tabs become draggable just like docked tabs.

### Floating chrome header

- Height: `FLOATING_HEADER_HEIGHT` (28px, matches tab bar)
- Collapse button: left side, clicks emit `LayoutAction::DockFloatingToGrid`
  - **Note:** This replaces egui's native `.collapsible(true)` (minimize to title bar) with a dock-to-grid action. This is a deliberate behavioral change — collapse means "return to grid," not "minimize."
- Title: center, shows active panel name
- Close button: right side, clicks emit `LayoutAction::CloseFloatingGroup`
- Drag on title area: moves the floating container position

### Floating container state management

Currently `egui::Window` manages position/size state internally. With `egui::Area`, we must manage this explicitly:

- **Position**: Updated on title bar drag. Stored back into `FloatingGroup.pos` each frame via `LayoutAction::UpdateFloatingGeometry`.
- **Size**: Updated on resize handle drag. Stored back into `FloatingGroup.size` via the same action.
- **Z-ordering**: Multiple floating panels need click-to-raise behavior. Track a `floating_z_order: Vec<GroupId>` on `DockLayout` (or use egui temp state). On pointer press inside a floating panel, move it to the front. Render floating panels in z-order so later panels paint on top.

### Floating container resize

Edge and corner sense zones around the `egui::Area` rect. On drag, emit `LayoutAction::UpdateFloatingGeometry` with new pos/size. Min size constraint: 200x100 (preserved from current behavior).

## File changes

### `src/ui/layout/render.rs`

- **Delete**: Floating group `egui::Window` block (lines 160-440) — inline tab bar, "+", grip, content, open-state tracking
- **Add**: `render_floating_chrome()` function — custom header + drag/resize + drop target rect registration + delegates to `render_tab_bar()` and `render_content()`
- **Add**: `FLOATING_HEADER_HEIGHT` constant
- **Add**: `LayoutAction::UpdateFloatingGeometry { group_id, pos, size }` variant (replaces separate move/resize actions)
- **No changes**: `render_tab_bar()`, `render_content()`, drag overlay logic

### `src/window.rs`

- **Add**: Handler for `LayoutAction::UpdateFloatingGeometry` in the action match block — updates `FloatingGroup.pos` and `FloatingGroup.size`

### `src/ui/layout/tree.rs`

- **Add**: `DockLayout::update_floating_geometry()` helper method to update a floating group's pos/size by group_id
- **Possibly add**: `floating_z_order: Vec<GroupId>` field on `DockLayout` for z-ordering (or use egui temp state instead)

### No changes needed

- `interactions.rs` — drop zone hit testing is rect-based
- `serialize.rs` — layout persistence unchanged (may need to persist z-order if added to DockLayout)

## Net effect

~240 lines of duplicated floating code deleted, replaced by ~100-120 lines of floating chrome rendering + state management. All panel group interactions become consistent across all contexts.
