# Exclude Lodestone Windows from Display Capture

**Date**: 2026-03-24

## Problem

When users add a full-display capture source, Lodestone's own windows (main window, settings, detached panels) appear in the recording/stream. Users expect the app to be invisible in its own output, like OBS.

Window capture already excludes Lodestone windows via PID-based filtering in `devices.rs`. Display capture has no exclusion capability because it uses `avfvideosrc`, which captures the entire screen without filtering.

## Solution

Replace `avfvideosrc`-based display capture with macOS ScreenCaptureKit, which natively supports excluding windows by PID. Add a user-facing toggle (default on) in General settings.

## Design

### 1. Setting

Add `exclude_self_from_capture: bool` to `GeneralSettings` in `src/settings.rs`, defaulting to `true`. Backward-compatible: the struct already has `#[serde(default)]`, so old config files deserialize fine.

Expose in the General settings UI section (`src/ui/settings/general.rs`) as a toggle labeled "Exclude Lodestone windows from display capture".

### 2. Settings Propagation

Add `exclude_self: bool` to `CaptureSourceConfig::Screen`:

```rust
pub enum CaptureSourceConfig {
    Screen { screen_index: u32, exclude_self: bool },
    Window { window_id: u32 },
    Camera { device_index: u32 },
}
```

This flows the setting through the existing `GstCommand::AddCaptureSource` channel. The UI reads `state.settings.general.exclude_self_from_capture` when constructing the config.

**Call sites that construct `CaptureSourceConfig::Screen`** (all need updating to pass the new field):
- `src/main.rs` — scene source startup on launch
- `src/ui/scenes_panel.rs` — `send_capture_for_scene()` / `apply_scene_diff()`
- `src/ui/sources_panel.rs` — `start_capture_from_properties()`
- `src/ui/preview_panel.rs` — capture source construction

Note: `src/ui/properties_panel.rs` modifies `SourceProperties::Display { screen_index }` but does not construct `CaptureSourceConfig` directly — re-capture is triggered indirectly via scene diff.

### 3. ScreenCaptureKit FFI Module

New file: `src/gstreamer/screencapturekit.rs`

Wraps the macOS ScreenCaptureKit APIs:
- `SCShareableContent` — enumerate displays and windows
- `SCContentFilter` — filter for a specific display, excluding windows by PID
- `SCStreamConfiguration` — resolution, pixel format, frame rate, cursor visibility
- `SCStream` + `SCStreamOutput` — capture stream, receive `CMSampleBuffer` frames
- Frame conversion: `CMSampleBuffer` pixel data to RGBA bytes

**Display mapping**: `screen_index: u32` maps to the corresponding entry in `SCShareableContent.displays` (an array of `SCDisplay`). Index 0 is the main display.

**Cursor capture**: `SCStreamConfiguration.showsCursor = true` to maintain parity with the current `avfvideosrc` behavior (`capture-screen-cursor: true`).

**Permission handling**: If `SCShareableContent` returns a permission error, send `GstError::CaptureFailure` over the error channel with a user-facing message ("Screen recording permission denied"). This matches the existing error pattern in `add_window_capture_source`.

Public API:

```rust
/// Start capturing a display, optionally excluding windows owned by our process.
pub fn start_display_capture(
    screen_index: u32,
    width: u32,
    height: u32,
    fps: u32,
    exclude_own_pid: bool,
) -> Result<(SCStreamHandle, Receiver<RgbaFrame>)>

/// Stop an active capture stream.
pub fn stop_display_capture(handle: SCStreamHandle) -> Result<()>
```

**Why a channel instead of direct `appsrc` push**: SCK callbacks (`stream:didOutputSampleBuffer:`) run on Apple's internal dispatch queues. Pushing directly to GStreamer `appsrc` from an arbitrary dispatch queue thread is unsafe. The channel decouples the ObjC callback from GStreamer threading, and the frame-pump thread on the GStreamer side pushes into `appsrc` safely — same indirection the existing window capture uses.

**Frame dimensions**: The `width` and `height` passed to `start_display_capture()` configure `SCStreamConfiguration.width/height`, which tells SCK to scale output to that size. Callers should pass the display's native resolution (or the user's configured base resolution from `VideoSettings`). The `appsrc` caps are set to match these dimensions.

Dependencies: `objc2` + `objc2-screen-capture-kit` + `objc2-core-media` crates for ScreenCaptureKit bindings, or raw `objc` FFI. Evaluate which approach is cleanest at implementation time.

### 4. Capture Pipeline Changes

**`src/gstreamer/capture.rs`** — modify `build_capture_pipeline()` for the `Screen` case:

Replace the `avfvideosrc` element with: `appsrc → videoconvert → videoscale → appsink`. No GStreamer screen capture element needed — frames come from ScreenCaptureKit via the channel. Note: `videorate` is intentionally omitted (unlike the old `avfvideosrc` pipeline) because frame timing is controlled by the SCK stream configuration and the pump thread, matching the window capture pattern.

**`src/gstreamer/thread.rs`** — modify `add_capture_source()` for `CaptureSourceConfig::Screen`:

- Call `screencapturekit::start_display_capture()` with screen index, dimensions, fps, and `exclude_self` flag
- Spawn a frame-pump thread that receives frames from the SCK channel and pushes into `appsrc` — identical pattern to existing `add_window_capture_source()`
- Store the `SCStreamHandle` in `CaptureHandle` via a new field: `sck_handle: Option<SCStreamHandle>`
- In `remove_capture_source()`, call `stop_display_capture()` on the handle before setting the pipeline to Null

**`CaptureHandle` changes**:
```rust
struct CaptureHandle {
    pipeline: gstreamer::Pipeline,
    appsink: gstreamer_app::AppSink,
    capture_running: Option<Arc<AtomicBool>>,
    sck_handle: Option<SCStreamHandle>,  // NEW
}
```

**Settings propagation**: The `exclude_self` bool flows via `CaptureSourceConfig::Screen`. Changing the setting at runtime does not affect active captures — it applies on next source creation or scene switch.

### 5. What Stays the Same

- Window capture (CoreGraphics direct) — unchanged
- Camera capture — unchanged
- Encoding, streaming, recording pipelines — unchanged (they receive frames from `appsink` regardless of source)
- Window enumeration PID filtering in `devices.rs` — unchanged

## Constraints

- macOS only (ScreenCaptureKit is macOS 12.3+). Linux/Windows display capture is not yet implemented and unaffected.
- Requires screen recording permission. Denied permission produces a `GstError::CaptureFailure` shown to the user.
- GStreamer still used for the pipeline (`appsrc → videoconvert → appsink`), just not for the capture source element.
