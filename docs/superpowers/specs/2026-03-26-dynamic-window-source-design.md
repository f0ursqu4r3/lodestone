# Dynamic Window Source

## Overview

Replace the static window-ID-based window capture with an application-tracking "dynamic" window source (StreamLabs-style). The source follows an application by bundle ID, automatically re-attaching when windows close/reopen. Includes an "Any Fullscreen Application" mode that captures whatever app is currently fullscreen (native or borderless).

## Data Model

Replace `SourceProperties::Window`:

```rust
SourceProperties::Window {
    /// Stable app identifier (e.g., "com.google.Chrome").
    /// None = "Any Fullscreen Application" mode.
    bundle_id: Option<String>,
    /// Human-readable app name for UI display.
    app_name: String,
    /// Pin to a specific window by title substring.
    /// None = auto-track frontmost window of the app.
    pinned_title: Option<String>,
    /// Runtime-only: currently tracked window ID. Not serialized.
    #[serde(skip)]
    current_window_id: Option<u32>,
}
```

Key shift: source identifies an **application** (bundle ID), not a window handle. `current_window_id` is transient runtime state.

## Window Watcher

Runs on the GStreamer thread. Polls window state every ~1-2 seconds via `SCShareableContent`.

### Resolution logic per source

**"Any Fullscreen App" mode** (`bundle_id: None`):
- Find any window whose bounds match a display's full frame (native fullscreen)
- Also detect borderless fullscreen: window covers entire screen without using native fullscreen
- Multiple fullscreen apps: prefer frontmost
- None fullscreen: hold last frame

**App-tracking mode** (`bundle_id: Some(...)`):
- Enumerate all windows belonging to that bundle ID
- `pinned_title` set: find window with matching title substring; fall back to frontmost if not found
- `pinned_title` None: pick frontmost/focused window of the app
- App not running: hold last frame, keep polling, auto-resume on relaunch

### Target switching

When resolved `window_id` changes:
- Call `SCStream::updateContentFilter` to swap the capture filter to the new `SCWindow` (no pipeline teardown)
- Update `current_window_id`
- If window size changed, update `native_size` and reconfigure stream dimensions

## ScreenCaptureKit Window Capture

Replace CoreGraphics polling with ScreenCaptureKit for window capture.

### New functions in `screencapturekit.rs`

- `start_window_capture(window_id, width, height, fps)` — uses `SCContentFilter::initWithDesktopIndependentWindow` to capture a single window
- `update_window_target(handle, new_window_id)` — live filter swap via `updateContentFilter`

### SCStreamHandle changes

```rust
pub struct SCStreamHandle {
    stream: Retained<SCStream>,
    _delegate: Retained<StreamOutputDelegate>,
    kind: CaptureKind, // Display { screen_index } | Window { window_id }
}
```

### Removals

- `grab_window_frame()` in `capture.rs` (CoreGraphics polling)
- `build_window_capture_pipeline()` (appsrc-based, replaced by SCK flow)
- Dedicated frame-grabbing thread per window

Window capture now uses the same SCK -> channel -> shared frame map flow as display capture.

## Window Enumeration Changes

### Enhanced `WindowInfo`

```rust
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
    pub bundle_id: String,
    pub bounds: (f64, f64, f64, f64), // x, y, width, height
    pub is_on_screen: bool,
}
```

Bundle ID extracted from `SCWindow.owningApplication.bundleIdentifier`. Bounds needed for fullscreen detection.

### Application grouping

New `enumerate_applications()` function that groups windows by bundle ID:

```rust
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
    pub windows: Vec<WindowInfo>,
}
```

## UI Changes (Properties Panel)

Replace the flat window dropdown with:

1. **Mode selector**: "Specific Application" | "Any Fullscreen Application"
2. **Application dropdown** (when Specific Application): grouped by app name, shows bundle ID subtitle. Populated from `enumerate_applications()`.
3. **Window pin toggle** (when app selected and has multiple windows): "Track frontmost window" vs "Pin to: [window title dropdown]"

## GStreamer Commands

New/modified commands:

- `CaptureSourceConfig::Window` changes from `{ window_id }` to `{ bundle_id: Option<String> }`
- Watcher resolves the actual window ID internally on the GStreamer thread

## Error States

- App not running: hold last frame, show "Waiting for [app name]" status in properties panel
- App has no capturable windows: same as not running
- Fullscreen mode with nothing fullscreen: hold last frame, show "No fullscreen application" status
- SCK permission denied: surface error to UI via existing error channel
