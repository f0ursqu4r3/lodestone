# Resolution Detection Design

**Date:** 2026-03-25
**Status:** Approved

## Problem

Lodestone defaults `base_resolution` to 1920x1080 and every source's `native_size` to (1920.0, 1080.0). Users with non-1080p monitors (e.g., 3360x1890) must manually change settings. Display and camera sources briefly render at the wrong size before the first frame corrects them.

## Decisions

- **Base resolution:** Auto-detect primary monitor on first launch only (no settings.toml). Once written, it's the user's setting.
- **Display sources:** Eager detection via SCDisplay bounds at source creation time.
- **Camera sources:** Query preferred resolution via GStreamer device monitor at enumeration time.
- **Resolution APIs:** Winit for base_resolution default (cross-platform), SCDisplay for display source native_size (macOS, already a dependency).
- **Settings UI:** Presets + detected resolutions + Custom input, replacing hard-coded dropdowns.

## Design

### 1. Base Resolution Auto-Detection

On first launch (no `settings.toml` exists), query winit's primary monitor via `event_loop.primary_monitor()` (falling back to `available_monitors().next()`). Call `.size()` to get physical resolution, format as the `base_resolution` default (e.g., `"3360x1890"`). Same value used for `output_resolution` default.

This happens in `main.rs` where `AppSettings::load_from()` is called. If the file doesn't exist, detect the monitor resolution instead of using the hard-coded 1920x1080 default. If the monitor can't be queried, fall back to 1920x1080.

Store the detected primary monitor resolution in `AppState` for use by the settings UI.

### 2. Display Source — Eager Resolution via SCDisplay

Extend display enumeration in `screencapturekit.rs` to return resolution (width, height) alongside screen index. The display info struct gains width/height fields from `SCDisplay.width()` and `SCDisplay.height()`.

When a display source is created, set `native_size` and initial transform from the SCDisplay bounds immediately — no waiting for the first frame.

The existing first-frame update in `main.rs` remains as a safety net but should rarely trigger for display sources.

### 3. Camera Source — Resolution at Enumeration

Extend `CameraDevice` in `devices.rs` to include `resolution: (u32, u32)`. During `enumerate_cameras()`, query GStreamer device monitor caps for each device and extract the highest supported resolution.

When a camera source is added, `native_size` and initial transform are set from this pre-queried resolution. If caps querying fails, fall back to (1920, 1080).

### 4. Resolution Settings UI

Replace hard-coded dropdowns in `ui/settings/video.rs` with a dynamic list:

1. **Detected resolutions** — primary monitor resolution from winit, labeled e.g. "3360x1890 (Display)"
2. **Common presets** — 1280x720, 1920x1080, 2560x1440, 3840x2160
3. **Custom...** — opens inline width/height number inputs

Deduplicate: if detected resolution matches a preset, show once with "(Display)" label. Sort by pixel count ascending. Applies to both base_resolution and output_resolution dropdowns.

### 5. Data Flow

```
App startup (main.rs)
  ├── winit: primary_monitor().size() → detected_resolution
  ├── If no settings.toml: base_resolution = detected_resolution
  ├── Store detected_resolution in AppState for settings UI
  └── GStreamer thread init
        └── enumerate_cameras() → Vec<CameraDevice> with resolution

User adds display source (library_panel.rs)
  └── SCDisplay.width()/height() → native_size + transform (immediate)

User adds camera source (library_panel.rs)
  └── CameraDevice.resolution → native_size + transform (immediate)

First frame arrives (main.rs, safety net)
  └── If native_size differs from frame → update (rarely triggers now)

Settings UI (video.rs)
  └── Dropdown: detected resolutions + presets + Custom...
```

No new threads, channels, or state management patterns. Each detection plugs into existing code at its natural point.

## Files Affected

- `src/settings.rs` — `VideoSettings::default()` detection path
- `src/main.rs` — winit monitor query at startup, pass to settings + AppState
- `src/state.rs` — `AppState` gains `detected_resolution: (u32, u32)` field
- `src/gstreamer/screencapturekit.rs` — display enumeration returns resolution
- `src/gstreamer/devices.rs` — `CameraDevice` gains resolution field, caps query
- `src/ui/library_panel.rs` — use eagerly-detected resolution when creating display/camera sources
- `src/ui/settings/video.rs` — dynamic resolution dropdown with presets + detected + custom
