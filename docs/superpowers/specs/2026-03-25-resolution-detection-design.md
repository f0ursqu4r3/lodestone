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
- **Resolution units:** All resolutions are in **logical points** (not physical pixels). This matches how GStreamer, ScreenCaptureKit, and the compositor work. Winit's `PhysicalSize` is converted to logical using the monitor's scale factor.
- **Window sources:** Out of scope for this design. Can be added later via CoreGraphics window bounds.

## Design

### 1. Base Resolution Auto-Detection

On first launch, query winit's primary monitor via `event_loop.primary_monitor()` (falling back to `available_monitors().next()`). Call `.size()` to get physical resolution, then divide by the monitor's scale factor (`.scale_factor()`) to get logical resolution. Format as the `base_resolution` default (e.g., `"3360x1890"`). Same value used for `output_resolution` default. Also apply to `StreamSettings` width/height defaults.

**First-launch detection:** Check whether the settings file exists (`path.exists()`) before calling `load_from()`. If the file does not exist, use the detected resolution. If the file exists but fails to parse, fall back to 1920x1080 (existing behavior). This distinguishes "no config yet" from "corrupt config."

If the monitor can't be queried, fall back to 1920x1080.

Store the detected primary monitor resolution in `AppState` as `detected_resolution: Option<(u32, u32)>`. `None` means detection failed — the settings UI won't show a "(Display)" label for a fallback value.

### 2. Display Source — Eager Resolution via SCDisplay

Create a new `enumerate_displays()` function in `screencapturekit.rs` that returns a `Vec<DisplayInfo>` with fields: index, width, height. Uses `SCDisplay.width()` and `SCDisplay.height()` which return logical points (not physical pixels on Retina). This matches what the capture pipeline produces — no scale factor conversion needed here.

When a display source is created in `library_panel.rs`, look up the `DisplayInfo` for the selected screen_index and set `native_size` and initial transform from its resolution immediately. The current hard-coded `(1920.0, 1080.0)` default in the source creation code must be replaced with the detected value, branching on source type.

The existing first-frame update in `main.rs` remains as a safety net but should rarely trigger for display sources.

### 3. Camera Source — Resolution at Enumeration

Extend `CameraDevice` in `devices.rs` to include `resolution: (u32, u32)`. During `enumerate_cameras()`, query GStreamer device monitor caps for each device and extract the highest supported resolution by pixel count (width * height). For range-style caps (e.g., `width = [1, 4096]`), use the maximum of the range.

When a camera source is added, `native_size` and initial transform are set from this pre-queried resolution. The source creation code in `library_panel.rs` branches on source type to use the camera's detected resolution instead of the hard-coded default. If caps querying fails, fall back to (1920, 1080).

### 4. Resolution Settings UI

Replace hard-coded dropdowns in `ui/settings/video.rs` with a dynamic list:

1. **Detected resolutions** — primary monitor resolution from winit (only shown if `detected_resolution` is `Some`), labeled e.g. "3360x1890 (Display)"
2. **Common presets** — 1280x720, 1920x1080, 2560x1440, 3840x2160
3. **Custom...** — reveals inline width and height number inputs. Values must be at least 2 and at most 7680, and must be even (encoder compatibility). Confirmed by pressing Enter or clicking away.

Deduplicate: if detected resolution matches a preset, show once with "(Display)" label. Sort by pixel count ascending. Applies to both base_resolution and output_resolution dropdowns.

### 5. Data Flow

```
App startup (main.rs)
  ├── winit: primary_monitor().size() / scale_factor() → detected_resolution
  ├── If no settings.toml: base_resolution, output_resolution, stream w/h = detected
  ├── Store detected_resolution in AppState (Option) for settings UI
  └── GStreamer thread init
        └── enumerate_cameras() → Vec<CameraDevice> with resolution

User adds display source (library_panel.rs)
  └── enumerate_displays() → DisplayInfo.width/height → native_size + transform

User adds camera source (library_panel.rs)
  └── CameraDevice.resolution → native_size + transform

First frame arrives (main.rs, safety net)
  └── If native_size differs from frame → update (rarely triggers now)

Settings UI (video.rs)
  └── Dropdown: detected resolutions + presets + Custom...
```

No new threads, channels, or state management patterns. Each detection plugs into existing code at its natural point.

## Files Affected

- `src/settings.rs` — first-launch detection path, `StreamSettings` defaults
- `src/main.rs` — winit monitor query at startup, pass to settings + AppState
- `src/state.rs` — `AppState` gains `detected_resolution: Option<(u32, u32)>` field
- `src/gstreamer/screencapturekit.rs` — new `enumerate_displays()` returning `Vec<DisplayInfo>`
- `src/gstreamer/devices.rs` — `CameraDevice` gains resolution field, caps query
- `src/ui/library_panel.rs` — branch on source type for native_size at creation time
- `src/ui/settings/video.rs` — dynamic resolution dropdown with presets + detected + custom
