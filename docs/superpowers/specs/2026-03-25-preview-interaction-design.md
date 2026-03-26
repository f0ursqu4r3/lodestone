# Preview Pane Interaction Refinements

## Overview

A comprehensive upgrade to the preview panel's interaction model: magnetic snapping, full grid/guide system, zoom/pan, copy/paste between scenes, multi-select, arrow key nudge, rotation, z-order shortcuts, snap toggle, and source locking.

## 1. Magnetic Snapping

### Problem

Current snapping uses a simple threshold check — if a source edge is within 8 canvas pixels of a target, it teleports to the target. There is no resistance when leaving, so slight mouse movement immediately unsnaps. This makes snapping feel weak and unreliable.

### Design

Replace with a two-zone magnetic snap model:

- **Attraction zone (outer): 12 canvas pixels.** When a source edge enters this zone during a drag, the source snaps to the target line.
- **Dead zone (inner): 4 canvas pixels.** Once snapped, mouse movement within this zone is absorbed — the source stays locked on the snap line. The user must drag past the dead zone boundary to break free.

Both zones scale proportionally with zoom level so snapping feels consistent regardless of zoom.

### Implementation

Add `snapped_x: Option<f32>` and `snapped_y: Option<f32>` to the drag state. When snapped on an axis:

1. Track the **raw** (unsnapped) mouse position separately from the displayed position.
2. On each frame, compare raw position to the snap line. If `|raw - snap_line| < dead_zone`, keep the source locked.
3. When raw position exceeds the dead zone, clear `snapped_x`/`snapped_y` and resume normal positioning.
4. When not snapped, check attraction zone as before to potentially re-snap.

Snap targets (unchanged): canvas edges (0, width, height), canvas center, grid lines, other source edges/centers, custom guides, rule-of-thirds lines, safe zone boundaries.

### Visual Feedback

Snap guide lines (existing) continue to render when snapped. They disappear when the snap breaks. No change to guide line appearance.

## 2. Grid & Guides System

### 2a. Visible Grid Overlay

- Thin lines rendered over the preview canvas, behind transform handles, above the preview texture.
- Two line weights: **major lines** (every 4th grid division) at higher opacity, **minor lines** at lower opacity.
- Grid renders in canvas space — it zooms and pans with the preview.
- Toggle via toolbar button or View menu. Default: off.
- At extreme zoom-out levels where grid lines would be < 4 screen pixels apart, hide the minor grid and only show major lines. If major lines are also too dense, hide the grid entirely.

### 2b. Grid Presets

Available presets selectable in settings or a toolbar popover:

| Preset | Grid Size |
|--------|-----------|
| 8px | 8 canvas pixels |
| 16px | 16 canvas pixels |
| 32px | 32 canvas pixels |
| 64px | 64 canvas pixels |
| Thirds | 1/3 and 2/3 of canvas width and height |
| Quarters | 1/4, 1/2, 3/4 of canvas width and height |
| Custom | User-specified value (existing `snap_grid_size` field) |

Pixel-based presets write to `snap_grid_size`. Thirds and Quarters are percentage-based modes that compute grid lines from `base_resolution`.

### 2c. Custom Draggable Guides

- Thin rulers rendered along the top and left edges of the preview panel when guides are enabled.
- Click and drag from a ruler into the canvas to place a guide at a specific canvas coordinate.
- Guides stored per-scene: `Vec<Guide>` on `Scene`, where:

```rust
pub struct Guide {
    pub axis: GuideAxis,    // Horizontal or Vertical
    pub position: f32,      // Canvas-space coordinate
}

pub enum GuideAxis {
    Horizontal,
    Vertical,
}
```

- Guides act as snap targets with the same priority as grid lines.
- Right-click a guide to delete it. "Clear All Guides" available in View menu.
- Guides render as colored dashed lines (default: cyan at 60% opacity).

### 2d. Rule-of-Thirds & Safe Zones

- **Rule-of-thirds:** Two horizontal + two vertical lines at 1/3 and 2/3 of canvas dimensions. Toggle independently from grid.
- **Safe zones:** Action-safe (90%) and title-safe (80%) rectangles — standard broadcast margins. Toggle independently.
- Both act as snap targets when visible.
- Both render in canvas space (zoom/pan aware).

### 2e. Color/Opacity Customization

New settings fields:

```rust
// In GeneralSettings or a new EditorSettings section
pub grid_color: [u8; 3],       // Default: [255, 255, 255]
pub grid_opacity: f32,          // Default: 0.15
pub guide_color: [u8; 3],       // Default: [0, 255, 255] (cyan)
pub guide_opacity: f32,         // Default: 0.60
```

## 3. Zoom & Pan

### Zoom

- **Scroll wheel:** Zoom centered on cursor position. Steps through: 10%, 25%, 33%, 50%, 67%, 75%, 100%, 150%, 200%, 300%, 400%, 600%, 800%.
- **Trackpad pinch:** Continuous cursor-centered zoom (not stepped).
- **Cmd+0:** Fit preview to panel (reset zoom and pan — equivalent to current default behavior).
- **Cmd+1:** 100% zoom (1 canvas pixel = 1 screen pixel), centered on current view center.
- Zoom level displayed in the preview overlay badges alongside resolution/fps.

### Pan

- **Middle-click drag:** Pan the preview surface.
- **Spacebar + left-click drag:** Temporary "hand tool" mode. Cursor changes to grab hand while spacebar is held.
- **Auto-pan:** When dragging a source near the edge of the visible area while zoomed in, the view slowly pans to follow.
- **Clamping:** At least 10% of the canvas must remain visible — can't pan the canvas entirely off-screen.

### State

New fields on the preview panel state (not persisted to settings — ephemeral per session):

```rust
pub zoom: f32,          // Multiplier on fit-to-panel scale. 1.0 = fit canvas to panel (default).
                        // 2.0 = 2x the fit-to-panel size. Cmd+1 calculates the zoom value
                        // that gives 1:1 canvas-to-screen pixel mapping.
pub pan_offset: Vec2,   // Canvas-space offset from center
```

All coordinate mapping functions (`canvas_to_screen`, `screen_to_canvas`) updated to account for zoom and pan. Transform handle hit areas remain constant in screen pixels (they don't shrink when zoomed out or grow when zoomed in).

Grid, guides, snap lines, safe zones, and rule-of-thirds all render correctly at any zoom level.

## 4. Copy / Paste / Duplicate

### Clipboard Model

App-internal clipboard (not system clipboard — source data is app-specific). Stored on `AppState`:

```rust
pub clipboard: Vec<ClipboardEntry>,
```

```rust
pub struct ClipboardEntry {
    pub library_source_id: SourceId,
    pub overrides_snapshot: SourceOverrides,
}
```

Clipboard persists across scene switches but not across app restarts.

### Operations

| Shortcut | Action |
|----------|--------|
| **Cmd+C** | Copy selected source(s) to clipboard. Stores library source ID + current scene overrides for each. |
| **Cmd+V** | Paste as reference. Adds the same library source to the active scene with the copied overrides. Transform offset by (+20, +20) canvas pixels to avoid exact overlap with the original. |
| **Cmd+Shift+V** | Paste as clone. Duplicates the library source (new ID, name suffixed " (Copy)"), adds the clone to the active scene with copied overrides. +20px offset. |
| **Cmd+D** | Duplicate in current scene. Shortcut for copy + paste-as-clone in one step. |

### Multi-select Aware

When multiple sources are selected, all are copied. Paste places all of them, preserving their relative positions. The offset (+20, +20) applies to the group's bounding box origin, not each source individually.

### Cross-scene Workflow

Copy in Scene A, switch to Scene B, paste. Works for both reference and clone modes. If pasting as reference and the library source is already in the target scene, the paste still adds a second instance (same library source can appear multiple times in a scene with different overrides).

## 5. Multi-Select

### Selection Mechanics

| Input | Behavior |
|-------|----------|
| **Click** source | Select that source only (deselect others). |
| **Shift+click** source | Toggle source in/out of current selection. |
| **Click empty space** | Deselect all. |
| **Cmd+A** | Select all visible, unlocked sources in active scene. |
| **Marquee drag** (click+drag on empty canvas) | Draw a selection rectangle. All sources intersecting the box get selected. |
| **Shift+marquee** | Add intersecting sources to existing selection. |

### Group Operations

| Operation | Multi-select Behavior |
|-----------|-----------------------|
| **Move** (drag) | All selected sources move together, preserving relative positions. Snapping applies to the dragged source (the "anchor"). |
| **Resize** (handle drag) | Only the source whose handle is dragged resizes. Other selections unaffected. |
| **Rotate** (Cmd+drag corner) | Only the source whose corner is dragged rotates. |
| **Delete** | All selected sources removed from scene. |
| **Copy/Paste** | All selected sources copied; pasted with relative positions preserved. |
| **Nudge** (arrow keys) | All selected sources move. |
| **Lock** (Cmd+L) | Toggle lock on all selected. |
| **Z-order** (Cmd+]/[) | All selected sources move in z-order. |

### Visual Feedback

- All selected sources show the selection outline (1px border).
- Only the most recently clicked source (the "primary" selection) shows resize/rotate handles. Others show outline only.

### State Change

```rust
// Before
pub selected_source_id: Option<SourceId>,

// After
pub selected_source_ids: Vec<SourceId>,
pub primary_selected_id: Option<SourceId>,  // For handle display
```

## 6. Arrow Key Nudge

- **Arrow keys:** Move selected source(s) by 1 canvas pixel.
- **Shift+arrow:** Move by 10 canvas pixels.
- Nudge runs through the snap system — after nudge, if the new position is within the attraction zone of a snap target, it snaps. Hold Alt during nudge to suppress snapping.
- **Undo batching:** Consecutive nudges within 500ms batch into a single undo entry to avoid flooding the undo stack.

## 7. Rotation

### Data Model

New field on `Transform`:

```rust
pub struct Transform {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation: f32,  // Degrees, default 0.0
}
```

### Interaction

- **Cmd+drag any corner handle** enters rotation mode. The corner handle acts as a rotation lever.
- Rotation pivots around the source's center point.
- **Shift constrains** to 15-degree increments (0, 15, 30, 45, ..., 345).
- Without Shift, rotation is free but shows a subtle angle indicator near 15-degree marks.
- Rotation value is editable in the properties panel (text field with degree suffix).

### Rendering & Hit-Testing

- Source rendering applies the rotation transform around the source center.
- Hit-testing for selection uses the rotated bounding box (oriented bounding box test, not AABB).
- Transform handle positions rotate with the source.
- Snap guide lines still reference the axis-aligned bounding box for edge snapping. Rotation snapping (angle-based) is separate from positional snapping.

## 8. Z-Order Shortcuts

| Shortcut | Action |
|----------|--------|
| **Cmd+]** | Move selected source(s) one layer forward in the scene's source list. |
| **Cmd+[** | Move one layer backward. |
| **Cmd+Shift+]** | Bring to front (top of source list). |
| **Cmd+Shift+[** | Send to back (bottom of source list). |

All z-order operations are undoable. When multiple sources are selected, they move as a group, preserving their relative order within the group.

## 9. Snap Toggle

- **Hold Alt** while dragging to temporarily suppress all snapping (grid, guide, edge, center — everything).
- Visual indicator: snap guide lines disappear, and a small "Snapping Off" indicator appears near the cursor or in the overlay badges.
- Also works during arrow key nudge (Alt+arrow = nudge without snapping).
- This is a temporary override — releasing Alt re-enables snapping. The `snap_to_grid` setting is not affected.

## 10. Source Locking

### Data Model

New field on `SourceOverrides`:

```rust
pub locked: Option<bool>,  // Per-scene override, default None (unlocked)
```

### Interaction

- **Lock icon** in the sources panel next to each source. Click to toggle.
- **Cmd+L** toggles lock on all selected source(s).
- **Locked sources:**
  - Cannot be moved, resized, or rotated via preview interaction.
  - Click still selects them (for property panel edits).
  - Transform handles render but are visually dimmed/grayed and non-interactive.
  - Skipped during marquee select (but can still be Shift+clicked explicitly).
  - Can still be deleted (lock prevents accidental drag, not intentional removal).

## Keyboard Shortcut Summary

| Shortcut | Action |
|----------|--------|
| Cmd+C | Copy selected source(s) |
| Cmd+V | Paste as reference |
| Cmd+Shift+V | Paste as independent clone |
| Cmd+D | Duplicate in current scene |
| Cmd+A | Select all |
| Cmd+L | Toggle lock |
| Cmd+] | Bring forward |
| Cmd+[ | Send backward |
| Cmd+Shift+] | Bring to front |
| Cmd+Shift+[ | Send to back |
| Cmd+0 | Fit to panel |
| Cmd+1 | 100% zoom |
| Arrow keys | Nudge 1px |
| Shift+Arrow | Nudge 10px |
| Alt (hold) | Suppress snapping |
| Cmd+drag corner | Rotate source |
| Shift (hold, while rotating) | Snap to 15-degree increments |
| Shift (hold, while resizing) | Lock aspect ratio |
| Space+drag | Pan preview |
| Middle-click drag | Pan preview |
| Scroll wheel | Zoom (cursor-centered) |
| Trackpad pinch | Zoom (cursor-centered) |

## Files Affected

| File | Changes |
|------|---------|
| `src/ui/transform_handles.rs` | Magnetic snap, rotation, multi-select handles, lock dimming, marquee select |
| `src/ui/preview_panel.rs` | Zoom/pan state, grid/guide rendering, rulers, zoom badges |
| `src/scene.rs` | `Transform.rotation`, `Guide` struct, `Vec<Guide>` on Scene, `SourceOverrides.locked` |
| `src/state.rs` | `selected_source_ids`, `primary_selected_id`, `clipboard`, zoom/pan state |
| `src/settings.rs` | Grid presets, grid/guide colors, safe zone toggles, rule-of-thirds toggle |
| `src/main.rs` | New keyboard shortcuts (copy/paste/duplicate, select all, lock, z-order, nudge, zoom reset) |
