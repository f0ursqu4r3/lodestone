# Window Picker (Dropper) for Window Source

## Overview

Add a "dropper" tool to the window source properties panel. When activated, a fullscreen transparent overlay covers the screen. As the user moves the mouse, the window under the cursor is highlighted. Clicking selects that window and configures the source to capture it. Escape cancels.

## Overlay Window

A borderless, transparent `winit` window covering the full screen at a high window level (above other apps). Uses a minimal wgpu render pass to draw a semi-transparent highlight rectangle over the detected window.

**Mouse tracking:** The overlay captures mouse move events via winit. On each move, call `CGWindowListCopyWindowInfo` to find the frontmost window under the cursor (excluding the overlay itself and Lodestone windows by PID). Extract that window's CGRect bounds and draw a highlight rectangle.

**Click:** On mouse down, capture the highlighted window's info (bundle_id, title, owner_name, window_id) from the CGWindowList data. Close the overlay and update the source's `WindowCaptureMode::Application` with the selected app's bundle_id, app_name, and optionally pinned_title. Properties panel updates and capture starts.

**Escape / right-click:** Cancel — close overlay, no changes.

**Cursor:** Crosshair while picker is active.

## Hit Testing

Use `CGWindowListCopyWindowInfo` with `kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements` to get window list ordered front-to-back. For each window:
- Skip windows owned by our PID (Lodestone + overlay)
- Skip windows with empty titles or below minimum size
- Check if mouse point is within the window's kCGWindowBounds
- First match = frontmost window under cursor

Extract bundle_id via the window's owning application PID → NSRunningApplication lookup.

## Integration

- A crosshair/dropper icon button in the properties panel, next to the application selector ComboBox (only visible in "Specific Application" mode)
- Clicking sets `AppState.window_picker_active = true` (or similar flag)
- The main event loop detects this flag and spawns the overlay
- On completion, the overlay writes the result to `AppState.window_picker_result: Option<PickerResult>`
- The properties panel reads and consumes this result on the next frame, updating the source

## Files

| File | Action | Responsibility |
|------|--------|---------------|
| `src/ui/window_picker.rs` | Create | Overlay window lifecycle, wgpu rendering, mouse tracking, CGWindow hit testing |
| `src/ui/properties_panel.rs` | Modify | Add dropper button, consume picker result |
| `src/state.rs` | Modify | Add `window_picker_active` flag and `window_picker_result` field |
| `src/main.rs` or `src/window.rs` | Modify | Detect picker flag, spawn overlay window |

## Picker Result

```rust
pub struct PickerResult {
    pub bundle_id: String,
    pub app_name: String,
    pub window_title: String,
    pub window_id: u32,
}
```
