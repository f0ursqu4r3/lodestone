# Interactive Transform Handles Design Spec

Add click-drag transform handles in the preview panel for repositioning and resizing sources visually.

## Design Decisions

- **Selection source:** Handles appear for whatever source is selected via `state.selected_source_id` (set by the Sources panel). No separate click-to-select in the preview — single source of truth.
- **Move + resize only.** No rotation or skew. Covers the core use case for streamers. Rotation can be added later.
- **Free resize by default.** Hold Shift to constrain aspect ratio. Matches Figma/Photoshop/OBS convention.

## Coordinate Mapping

The preview panel renders the compositor output in a letterboxed viewport. Transform handles must convert between canvas space and screen space.

### Canvas Space

Source transforms are in pixel coordinates on the compositor canvas (e.g., 0..1920 x 0..1080). `Transform { x, y, width, height }` is in this space.

### Screen Space

The preview viewport is a sub-rect of the egui panel, possibly letterboxed. All egui input (pointer position, drag deltas) is in screen pixels.

### Conversion Functions

```rust
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
```

These account for the viewport offset (letterboxing) and the scale ratio between canvas resolution and viewport pixel size.

## Handle Types

### Body Drag (Move)

Click inside the source rect (not on a handle) and drag to reposition. Updates `transform.x` and `transform.y`.

### Corner Handles (Resize)

Four 8x8px squares at each corner of the source rect. Drag to resize from that corner. The opposite corner stays pinned.

| Handle | Pinned Corner | Cursor |
|--------|--------------|--------|
| Top-Left | Bottom-Right | `nw-resize` |
| Top-Right | Bottom-Left | `ne-resize` |
| Bottom-Left | Top-Right | `sw-resize` |
| Bottom-Right | Top-Left | `se-resize` |

**Aspect ratio:** Free resize by default. When Shift is held, constrain to the source's original aspect ratio (width/height at drag start).

### Edge Handles (Single-Axis Resize)

Four 6x6px squares at edge midpoints. Drag to resize on one axis only.

| Handle | Axis | Cursor |
|--------|------|--------|
| Top | Height (top edge moves) | `n-resize` |
| Bottom | Height (bottom edge moves) | `s-resize` |
| Left | Width (left edge moves) | `w-resize` |
| Right | Width (right edge moves) | `e-resize` |

## Interaction State

Stored in egui's temporary memory per source ID:

```rust
#[derive(Clone)]
enum HandlePosition {
    TopLeft, Top, TopRight,
    Left, Right,
    BottomLeft, Bottom, BottomRight,
}

#[derive(Clone)]
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
```

### Drag Flow

1. **Mouse down** on a handle or source body → set `DragMode` with starting positions
2. **Mouse move** while dragging → compute delta in canvas space, apply to transform
3. **Mouse up** → clear `DragMode`, mark `state.scenes_dirty = true`

### Resize with Aspect Lock (Shift)

When Shift is held during a corner resize:
- Compute the new size freely from the drag delta
- Then constrain: `new_height = new_width / aspect_ratio`
- The pinned corner stays fixed, the dragged corner adjusts

Edge handles ignore Shift — they're single-axis by definition.

### Minimum Size

Enforce a minimum source size of 10x10 canvas pixels to prevent collapsing to zero.

## Visual Appearance

All drawing uses the painter overlay on top of the preview viewport.

- **Selection outline:** 1px `TEXT_PRIMARY` border around the source rect (in screen space)
- **Corner handles:** 8x8px filled rect, `TEXT_PRIMARY` fill, 1px `BG_BASE` stroke. Centered on the corner point.
- **Edge handles:** 6x6px filled rect, same styling. Centered on the edge midpoint.
- **Hover:** Handle being hovered gets a slightly larger size (10x10 for corners, 8x8 for edges) for visual feedback.
- **While dragging:** Draw a 1px dashed outline showing the new rect position/size in `TEXT_SECONDARY`.

## Hit Testing

Handle hit areas are larger than their visual size for easier targeting:
- Corner handles: 16x16px hit area (centered on the 8x8 visual)
- Edge handles: 12x12px hit area
- Body: the full source rect minus handle areas

Priority: handles win over body (clicking near a corner resizes, not moves).

## Integration with Properties Panel

Transform handle edits write directly to `source.transform` in `state.sources`. The Properties panel reads the same fields — so dragging a source in the preview updates the X/Y/W/H values in Properties in real-time. No special sync needed.

## File Structure

```
src/ui/transform_handles.rs  # NEW — handle drawing, hit testing, drag logic, coord mapping
src/ui/preview_panel.rs      # MODIFY — call draw_transform_handles after preview render
src/ui/mod.rs                # MODIFY — add transform_handles module
```

All transform handle logic lives in `transform_handles.rs`. The preview panel calls a single entry point:

```rust
pub fn draw_transform_handles(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewport_rect: Rect,
    canvas_size: Vec2,
)
```

This function:
1. Reads `state.selected_source_id`
2. Finds the source's transform
3. Converts to screen space
4. Draws handles
5. Handles input (drag start/move/end)
6. Writes back to `source.transform`
