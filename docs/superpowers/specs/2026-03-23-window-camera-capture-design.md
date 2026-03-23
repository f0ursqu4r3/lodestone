# Window & Camera Capture Sources Design Spec

Add window-level and camera capture as new source types, extending the existing display capture system.

## Design Decisions

- **Window capture:** Platform API (`CoreGraphics` / `ScreenCaptureKit`) for precise window targeting, frames fed into GStreamer via `appsrc`. GStreamer's `avfvideosrc` doesn't expose window-level targeting reliably.
- **Camera capture:** GStreamer's `avfvideosrc` without `capture-screen: true`, device selected by index. Cameras are a solved problem in GStreamer — no need for custom platform code.
- **Window selection UI:** Dropdown ComboBox listing windows by "App — Title", with a refresh button. Future enhancement: visual thumbnail picker.
- **Camera selection UI:** Dropdown ComboBox listing cameras by device name, runtime-enumerated.

## Data Model

### SourceProperties (src/scene.rs)

Add two new variants to the existing enum:

```rust
pub enum SourceProperties {
    Display { screen_index: u32 },
    Window { window_id: u32, window_title: String, owner_name: String },
    Camera { device_index: u32, device_name: String },
}
```

`window_title` and `owner_name` are stored for display purposes. The `window_id` (`CGWindowID`) is the actual identifier used for capture. `device_name` is stored so the Properties panel can show which camera is selected without re-enumerating.

### CaptureSourceConfig (src/gstreamer/commands.rs)

Add two new variants:

```rust
pub enum CaptureSourceConfig {
    Screen { screen_index: u32 },
    Window { window_id: u32 },
    Camera { device_index: u32 },
}
```

These are the minimal configs needed to build capture pipelines. UI-facing metadata (window title, camera name) stays in `SourceProperties`, not here.

### New State Fields (src/state.rs)

```rust
pub available_cameras: Vec<CameraDevice>,
pub available_windows: Vec<WindowInfo>,
```

### New Types (src/gstreamer/devices.rs)

```rust
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
}

pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
}
```

## Window Capture Pipeline

### Enumeration

`enumerate_windows()` in `src/gstreamer/devices.rs`:

- Uses `CGWindowListCopyWindowInfo` via the `core-graphics` crate (or raw FFI bindings if the crate doesn't expose this function — verify at implementation time).
- Returns `Vec<WindowInfo>` with window ID, title, and owning application name.
- Filters out: windows with empty titles, windows owned by Lodestone itself, windows below a minimum size (e.g., 50x50).
- Called on demand when the user opens the window selector or clicks refresh. Not polled automatically.

### Capture

`build_window_capture_pipeline()` in `src/gstreamer/capture.rs`:

- Uses `CGWindowListCreateImage(CGRectNull, kCGWindowListOptionIncludingWindow, window_id, kCGWindowImageBoundsIgnoreFraming)` to capture a single window.
- Runs on a dedicated thread **per window source** (separate from the GStreamer thread) with a configurable frame interval (default: 1/30s for 30fps, or 1/60s for 60fps — match the output FPS from settings). This is necessary because `appsrc` requires the caller to drive the frame loop, unlike `avfvideosrc` where GStreamer manages its own thread.
- Converts the `CGImage` to raw RGBA bytes.
- Pushes frames into a GStreamer `appsrc` element configured with RGBA caps at the window's resolution.
- Pipeline: `appsrc → videoconvert → videoscale → appsink` (same tail as display capture).
- The `appsrc` is configured with `is-live: true`, `format: time`, `do-timestamp: true`.

### Window Closure Handling

- If `CGWindowListCreateImage` returns null (window closed/minimized), the capture thread:
  1. Sends a `GstError::CaptureFailure` with "Window closed or unavailable"
  2. Pushes the last valid frame one more time (compositor holds it)
  3. Stops the capture loop
- The UI shows the error in `state.active_errors`. The source remains in the scene but stops updating.

**Note on persistence:** Window IDs (`CGWindowID`) are not stable across app restarts. A saved scene referencing a window source will have a stale ID on reload. The window source will require re-selection after Lodestone restarts. Future enhancement: match by owner + title to auto-re-acquire.

## Camera Capture Pipeline

### Enumeration

`enumerate_cameras()` in `src/gstreamer/devices.rs`:

- Uses GStreamer's `DeviceMonitor` with `Video/Source` class filter (same pattern as existing audio device enumeration).
- Returns `Vec<CameraDevice>` with device index and display name.
- Called once at startup and re-enumerable on demand.
- Filters out screen capture devices (those with `capture-screen` capability).

### Capture

`build_camera_capture_pipeline()` in `src/gstreamer/capture.rs`:

- Uses `avfvideosrc` with:
  - `device-index: u32` (from `CaptureSourceConfig::Camera`)
  - No `capture-screen` property set (defaults to false)
- Pipeline: `avfvideosrc → videoconvert → videoscale → videorate → appsink` (identical to display capture, minus `capture-screen: true`).
- Resolution and framerate negotiated via caps, same as display capture.

### Device Disconnect Handling

- If the camera device is disconnected, GStreamer posts an error on the bus.
- The GStreamer thread's bus watcher catches it and sends `GstError::CaptureFailure`.
- Same behavior as window closure — source stays, stops updating, error shown.

## UI Changes

### Sources Panel (src/ui/sources_panel.rs)

The "+" button currently adds a display source directly. Change to show a source type picker:

- Click "+" → small popup/menu with: "Display", "Window", "Camera"
- Selecting a type creates the source with default properties:
  - Display: `screen_index: 0` (first monitor)
  - Window: opens the window selector in Properties immediately, no capture until a window is chosen
  - Camera: `device_index: 0` (first camera), or prompts if no cameras found

### Properties Panel (src/ui/properties_panel.rs)

Extend the source-type match in the SOURCE section:

**Window source:**
- "Window" ComboBox: lists `state.available_windows` as "Owner — Title"
- "Refresh" button next to it: calls window enumeration, updates `state.available_windows`
- Selecting a window updates `source.properties` and sends `RemoveCaptureSource` + `AddCaptureSource` with the new window ID

**Camera source:**
- "Camera" ComboBox: lists `state.available_cameras` by name
- Selecting a camera updates `source.properties` and sends `RemoveCaptureSource` + `AddCaptureSource` with the new device index

### Source Icons (src/ui/sources_panel.rs)

Already mapped in `source_icon()`:
- `SourceType::Window` → `egui_phosphor::regular::APP_WINDOW`
- `SourceType::Camera` → `egui_phosphor::regular::VIDEO_CAMERA`

## Dependencies

### New Crate

`core-graphics` — for `CGWindowListCopyWindowInfo` and `CGWindowListCreateImage`. macOS only, behind `#[cfg(target_os = "macos")]`.

### Existing Crates (no changes)

- `gstreamer` / `gstreamer-app` — already used for `appsrc`/`appsink`
- `egui_phosphor` — icons already mapped

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Window closed during capture | `CaptureFailure` error, last frame held, capture stops |
| Window minimized | Platform API may return empty/black frame — push it as-is |
| Camera disconnected | GStreamer bus error → `CaptureFailure`, capture stops |
| No cameras found | "No cameras available" in Properties ComboBox, disabled |
| macOS Screen Recording permission denied | `PermissionDenied` error (existing handling) |
| Window enumeration fails | Return empty list, log warning |

## File Structure

```
src/scene.rs                  # MODIFY — SourceProperties::Window, SourceProperties::Camera
src/state.rs                  # MODIFY — available_cameras, available_windows fields
src/gstreamer/commands.rs     # MODIFY — CaptureSourceConfig::Window, Camera
src/gstreamer/capture.rs      # MODIFY — build_window_capture_pipeline, build_camera_capture_pipeline
src/gstreamer/devices.rs      # MODIFY — enumerate_cameras(), enumerate_windows()
src/gstreamer/thread.rs       # MODIFY — handle new config variants in add_capture_source
src/ui/sources_panel.rs       # MODIFY — source type picker (Display/Window/Camera)
src/ui/properties_panel.rs    # MODIFY — window selector, camera selector
```

## Future Enhancements

- **Visual window picker** — Grid of window thumbnails (like macOS Mission Control) instead of a dropdown. Requires `CGWindowListCreateImage` thumbnails rendered in egui.
- **Camera preview in Properties** — Live preview of the camera feed in the Properties panel before adding to scene.
- **Hot-plug detection** — Automatically update camera list when devices connect/disconnect via GStreamer DeviceMonitor bus messages.
- **Window tracking** — Re-acquire window by title/owner if the window ID changes (app restart).
