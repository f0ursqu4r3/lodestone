# Resolution Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-detect display and camera resolutions so the app defaults to correct sizes instead of hard-coded 1920x1080.

**Architecture:** Display enumeration via SCDisplay (already available at startup) provides both the primary monitor resolution for first-launch settings and per-display resolution for source creation. GStreamer device monitor provides camera resolution at enumeration. Settings UI offers detected + presets + custom.

**Tech Stack:** Rust, winit (monitor scale factor), objc2_screen_capture_kit (SCDisplay), gstreamer (camera caps), egui (settings UI)

**Spec:** `docs/superpowers/specs/2026-03-25-resolution-detection-design.md`

---

### Task 1: Display enumeration with resolution

**Files:**
- Modify: `src/gstreamer/screencapturekit.rs` (add `DisplayInfo` struct and `enumerate_displays()`)
- Modify: `src/gstreamer/mod.rs` (re-export `DisplayInfo`)

- [ ] **Step 1: Add DisplayInfo struct and enumerate_displays()**

In `src/gstreamer/screencapturekit.rs`, add after the `SCStreamHandle` struct (around line 36):

```rust
/// Info about a display available for capture, including its native resolution
/// in logical points.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub index: usize,
    pub width: u32,
    pub height: u32,
}

/// Enumerate available displays and their native resolutions via ScreenCaptureKit.
///
/// Returns a list of displays with their logical-point dimensions.
/// Uses `SCDisplay.width()` and `SCDisplay.height()` which return logical points
/// (not physical pixels on Retina displays).
pub fn enumerate_displays() -> Result<Vec<DisplayInfo>> {
    let content = get_shareable_content()?;
    let displays: Retained<NSArray<SCDisplay>> = unsafe { content.displays() };
    let count = displays.count();
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let display = unsafe { displays.objectAtIndex_unchecked(i) };
        let width = unsafe { display.width() } as u32;
        let height = unsafe { display.height() } as u32;
        result.push(DisplayInfo {
            index: i,
            width,
            height,
        });
    }
    Ok(result)
}
```

- [ ] **Step 2: Re-export DisplayInfo from gstreamer module**

In `src/gstreamer/mod.rs`, add `DisplayInfo` to the public re-exports from `screencapturekit`.

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/screencapturekit.rs src/gstreamer/mod.rs
git commit -m "feat: add enumerate_displays() returning per-display resolution via SCDisplay"
```

---

### Task 2: First-launch resolution detection and AppState field

**Files:**
- Modify: `src/state.rs:143` (add `detected_resolution` and `available_displays` fields)
- Modify: `src/settings.rs:257-264` (add `load_or_detect` method)
- Modify: `src/main.rs:175-221` (enumerate displays at startup, use for settings + state)

The key insight: `AppManager::new()` already calls blocking functions like `enumerate_cameras()` and `enumerate_windows()` before the event loop exists (lines 187-198). We add `enumerate_displays()` in the same place. The primary display's resolution from `DisplayInfo` serves as the detected resolution — no need for winit at this stage (SCDisplay returns logical points, which is what we want). Winit's `scale_factor()` is only needed if we were using physical pixels, but SCDisplay already gives logical.

- [ ] **Step 1: Add fields to AppState**

In `src/state.rs`, add after `monitor_count: usize` (line 143):

```rust
    /// Primary monitor resolution detected at startup via SCDisplay, in logical points.
    /// `None` if detection failed. Used by settings UI to offer detected resolution.
    pub detected_resolution: Option<(u32, u32)>,
    /// Available displays with resolution info, populated at startup.
    pub available_displays: Vec<crate::gstreamer::DisplayInfo>,
```

These default to `None` and `Vec::new()` respectively in `AppState::default()`.

- [ ] **Step 2: Add `load_or_detect` method to AppSettings**

In `src/settings.rs`, add after `load_from` (line 264):

```rust
    /// Load settings from disk. On first launch (no settings file), use the
    /// detected monitor resolution for video/stream defaults instead of 1920x1080.
    pub fn load_or_detect(path: &Path, detected: Option<(u32, u32)>) -> Self {
        if path.exists() {
            // File exists — load it (falls back to defaults on parse error)
            match std::fs::read_to_string(path) {
                Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else if let Some((w, h)) = detected {
            // First launch with detected monitor resolution
            let res_str = format!("{w}x{h}");
            let mut settings = Self::default();
            settings.video.base_resolution = res_str.clone();
            settings.video.output_resolution = res_str;
            settings.stream.width = w;
            settings.stream.height = h;
            settings
        } else {
            // First launch, detection failed — use defaults
            Self::default()
        }
    }
```

- [ ] **Step 3: Enumerate displays and use detected resolution at startup**

In `src/main.rs`, add display enumeration in `AppManager::new()` after window enumeration (after line 198):

```rust
        // Enumerate displays for resolution detection.
        let available_displays = {
            #[cfg(target_os = "macos")]
            {
                match crate::gstreamer::screencapturekit::enumerate_displays() {
                    Ok(displays) => {
                        log::info!("Found {} display(s)", displays.len());
                        displays
                    }
                    Err(e) => {
                        log::warn!("Failed to enumerate displays: {e}");
                        Vec::new()
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            { Vec::new() }
        };
        let detected_resolution = available_displays.first().map(|d| (d.width, d.height));
```

Then replace line 208:
```rust
        let saved_settings = settings::AppSettings::load_from(&settings::settings_path());
```
With:
```rust
        let saved_settings = settings::AppSettings::load_or_detect(
            &settings::settings_path(),
            detected_resolution,
        );
```

And update the `initial_state` construction (lines 210-221) to include the new fields:
```rust
        let initial_state = AppState {
            scenes: collection.scenes,
            library: collection.library,
            active_scene_id: collection.active_scene_id,
            next_scene_id: collection.next_scene_id,
            next_source_id: collection.next_source_id,
            command_tx: Some(main_channels.command_tx.clone()),
            available_cameras,
            available_windows,
            available_displays,
            detected_resolution,
            settings: saved_settings,
            ..AppState::default()
        };
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 5: Test manually**

Delete `~/.config/lodestone/settings.toml`, run the app, then check:
```bash
cat ~/.config/lodestone/settings.toml | grep -A3 "\[video\]"
```
Expected: `base_resolution` and `output_resolution` match your primary display's logical resolution.

- [ ] **Step 6: Commit**

```bash
git add src/state.rs src/settings.rs src/main.rs
git commit -m "feat: auto-detect monitor resolution on first launch for base/output/stream settings"
```

---

### Task 3: Camera enumeration with resolution

**Files:**
- Modify: `src/gstreamer/devices.rs:6-11` (add resolution field to `CameraDevice`)
- Modify: `src/gstreamer/devices.rs:153-189` (query caps during enumeration)

- [ ] **Step 1: Add resolution field to CameraDevice**

In `src/gstreamer/devices.rs`, update the struct (lines 7-11):

```rust
/// A camera device discovered via GStreamer DeviceMonitor.
#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
    /// Native resolution (width, height) from device caps. Falls back to (1920, 1080).
    pub resolution: (u32, u32),
}
```

- [ ] **Step 2: Add a helper to extract max resolution from device caps**

Add before `enumerate_cameras()`:

```rust
/// Extract the highest resolution (by pixel count) from a GStreamer device's caps.
///
/// Handles fixed values. Returns `None` if no usable video caps found.
fn max_resolution_from_device(device: &gstreamer::Device) -> Option<(u32, u32)> {
    let caps = device.caps()?;
    let mut best: Option<(u32, u32)> = None;
    for s in caps.iter() {
        let w = match s.get::<i32>("width") {
            Ok(v) => v as u32,
            Err(_) => continue,
        };
        let h = match s.get::<i32>("height") {
            Ok(v) => v as u32,
            Err(_) => continue,
        };
        let pixels = w as u64 * h as u64;
        if best.map_or(true, |(bw, bh)| pixels > bw as u64 * bh as u64) {
            best = Some((w, h));
        }
    }
    best
}
```

Note: `get::<i32>("width")` works for fixed-value caps. For range-style caps (e.g., virtual cameras), this returns `Err` and the structure is skipped — the function falls through to `None`, and the caller uses the (1920, 1080) fallback. This is acceptable; range-cap cameras are rare and the first-frame update in `main.rs` will correct the size.

- [ ] **Step 3: Update enumerate_cameras() to populate resolution**

Replace the `.map` closure in `enumerate_cameras()` (lines 163-170):

```rust
    let all: Vec<CameraDevice> = devices
        .iter()
        .enumerate()
        .map(|(i, device)| {
            let resolution = max_resolution_from_device(&device).unwrap_or((1920, 1080));
            CameraDevice {
                device_index: i as u32,
                name: device.display_name().to_string(),
                resolution,
            }
        })
        .collect();
```

- [ ] **Step 4: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 5: Commit**

```bash
git add src/gstreamer/devices.rs
git commit -m "feat: query camera resolution from GStreamer device caps at enumeration"
```

---

### Task 4: Eager native_size for display and camera sources

**Files:**
- Modify: `src/ui/library_panel.rs:275-285` (display source uses detected resolution)
- Modify: `src/ui/library_panel.rs:302-314` (camera source uses detected resolution)
- Modify: `src/ui/library_panel.rs:395-408` (parameterize native_size/transform)

- [ ] **Step 1: Compute per-source native size before LibrarySource construction**

In `src/ui/library_panel.rs`, the source type match (starting ~line 275) builds `(name, properties)`. After the match but before `LibrarySource` construction (line 395), compute the native size based on source type:

```rust
    // Determine native size from detected resolution for display/camera,
    // or use default 1920x1080 for other source types.
    let (native_w, native_h) = match &properties {
        SourceProperties::Display { screen_index } => {
            state
                .available_displays
                .iter()
                .find(|d| d.index == *screen_index as usize)
                .map(|d| (d.width as f32, d.height as f32))
                .unwrap_or((1920.0, 1080.0))
        }
        SourceProperties::Camera { device_index, .. } => {
            state
                .available_cameras
                .iter()
                .find(|c| c.device_index == *device_index)
                .map(|c| (c.resolution.0 as f32, c.resolution.1 as f32))
                .unwrap_or((1920.0, 1080.0))
        }
        _ => (1920.0, 1080.0),
    };
```

- [ ] **Step 2: Use computed native size in LibrarySource construction**

Replace the hard-coded values at lines 395-408:

```rust
    let lib_source = LibrarySource {
        id: new_id,
        name,
        source_type,
        properties,
        folder: None,
        transform: Transform::new(0.0, 0.0, native_w, native_h),
        native_size: (native_w, native_h),
        aspect_ratio_locked: false,
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Test manually**

Add a display source. Verify it appears at the correct native resolution in the preview immediately (no visual snap from 1920x1080).

- [ ] **Step 5: Commit**

```bash
git add src/ui/library_panel.rs
git commit -m "feat: set native_size from detected resolution when creating display/camera sources"
```

---

### Task 5: Dynamic resolution settings UI

**Files:**
- Modify: `src/ui/settings/video.rs` (replace hard-coded dropdowns, add custom input)
- Modify: `src/ui/settings/mod.rs:222` (update call site to pass detected_resolution)

- [ ] **Step 1: Update video::draw() signature and call site**

Change the signature in `src/ui/settings/video.rs` (line 7):

```rust
pub(super) fn draw(
    ui: &mut Ui,
    settings: &mut VideoSettings,
    detected_resolution: Option<(u32, u32)>,
) -> bool {
```

Update the call site in `src/ui/settings/mod.rs` (line 222). The `draw_settings` function receives `state: &mut AppSettings` but `detected_resolution` is on `AppState`, not `AppSettings`. Looking at the call chain: `draw_settings` at line 222 passes `&mut state.settings.video`. The caller of `draw_settings` has `AppState`.

Check how `draw_settings` is called — it likely receives `&mut AppState` via `state`. Update line 222:

```rust
SettingsCategory::Video => video::draw(ui, &mut state.settings.video, state.detected_resolution),
```

If `state` here is `AppState` (not `AppSettings`), this works directly. If `state` is `AppSettings`, we need to thread `detected_resolution` through. Read the actual call chain to determine.

- [ ] **Step 2: Build resolution options helper**

In `src/ui/settings/video.rs`, add before `draw()`:

```rust
struct ResolutionOption {
    value: String,
    label: String,
}

fn build_resolution_options(
    detected: Option<(u32, u32)>,
    include_720p: bool,
) -> Vec<ResolutionOption> {
    let presets: Vec<(u32, u32)> = if include_720p {
        vec![(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)]
    } else {
        vec![(1920, 1080), (2560, 1440), (3840, 2160)]
    };

    let mut options: Vec<ResolutionOption> = presets
        .iter()
        .map(|&(w, h)| {
            let value = format!("{w}x{h}");
            let is_detected = detected == Some((w, h));
            let label = if is_detected {
                format!("{value} (Display)")
            } else {
                value.clone()
            };
            ResolutionOption { value, label }
        })
        .collect();

    // Insert detected resolution in sorted position if not already a preset
    if let Some((w, h)) = detected {
        if !presets.contains(&(w, h)) {
            let value = format!("{w}x{h}");
            let label = format!("{value} (Display)");
            let pixels = w as u64 * h as u64;
            let pos = options
                .iter()
                .position(|o| {
                    let (ow, oh) = crate::renderer::compositor::parse_resolution(&o.value);
                    (ow as u64 * oh as u64) > pixels
                })
                .unwrap_or(options.len());
            options.insert(pos, ResolutionOption { value, label });
        }
    }

    options
}
```

- [ ] **Step 3: Replace the base resolution dropdown**

Replace lines 12-30 with:

```rust
    let base_options = build_resolution_options(detected_resolution, false);
    let is_custom_base = !base_options.iter().any(|o| o.value == settings.base_resolution);
    let base_display_text = if is_custom_base {
        format!("Custom ({})", settings.base_resolution)
    } else {
        base_options
            .iter()
            .find(|o| o.value == settings.base_resolution)
            .map(|o| o.label.clone())
            .unwrap_or_else(|| settings.base_resolution.clone())
    };

    ui.horizontal(|ui| {
        labeled_row(ui, "Base (Canvas) Resolution");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let combo = egui::ComboBox::from_id_salt("base_res")
                .selected_text(&base_display_text)
                .show_ui(ui, |ui| {
                    let mut c = false;
                    for opt in &base_options {
                        c |= ui
                            .selectable_value(
                                &mut settings.base_resolution,
                                opt.value.clone(),
                                &opt.label,
                            )
                            .changed();
                    }
                    // Custom option — sets resolution to "custom" sentinel,
                    // then the DragValue row below handles actual input.
                    if ui.selectable_label(is_custom_base, "Custom...").clicked() && !is_custom_base {
                        settings.base_resolution = "custom".to_string();
                        c = true;
                    }
                    c
                });
            if let Some(inner) = combo.inner {
                changed |= inner;
            }
        });
    });

    // Custom resolution input for base resolution
    if is_custom_base || settings.base_resolution == "custom" {
        if settings.base_resolution == "custom" {
            // Initialize to current parsed value or default
            settings.base_resolution = "1920x1080".to_string();
            changed = true;
        }
        ui.horizontal(|ui| {
            labeled_row(ui, "");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (mut w, mut h) =
                    crate::renderer::compositor::parse_resolution(&settings.base_resolution);
                let w_changed = ui
                    .add(egui::DragValue::new(&mut w).range(2..=7680).suffix("w"))
                    .changed();
                ui.label("x");
                let h_changed = ui
                    .add(egui::DragValue::new(&mut h).range(2..=7680).suffix("h"))
                    .changed();
                if w_changed || h_changed {
                    // Ensure even values for encoder compatibility
                    w = (w / 2) * 2;
                    h = (h / 2) * 2;
                    w = w.max(2);
                    h = h.max(2);
                    settings.base_resolution = format!("{w}x{h}");
                    changed = true;
                }
            });
        });
    }
```

- [ ] **Step 4: Replace the output resolution dropdown**

Apply the same pattern to the output resolution dropdown (lines 32-54), using `include_720p: true`:

```rust
    let output_options = build_resolution_options(detected_resolution, true);
    let is_custom_output = !output_options.iter().any(|o| o.value == settings.output_resolution);
    // ... same pattern as base resolution, with "output_res" id salt
```

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 6: Test the UI manually**

Open Settings > Video. Verify:
- Detected monitor resolution appears with "(Display)" label
- Common presets are listed, sorted by pixel count
- "Custom..." sets to editable width/height DragValues
- Custom values clamp to even numbers in range 2-7680

- [ ] **Step 7: Commit**

```bash
git add src/ui/settings/video.rs src/ui/settings/mod.rs
git commit -m "feat: dynamic resolution dropdowns with detected, presets, and custom input"
```

---

### Task 6: Tests and cleanup

**Files:**
- Modify: `src/settings.rs` (add tests for `load_or_detect`)

- [ ] **Step 1: Write tests for load_or_detect**

In `src/settings.rs`, add to the existing `#[cfg(test)]` module:

```rust
#[test]
fn load_or_detect_first_launch_uses_detected_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    // File does not exist — should use detected resolution
    let settings = AppSettings::load_or_detect(&path, Some((3360, 1890)));
    assert_eq!(settings.video.base_resolution, "3360x1890");
    assert_eq!(settings.video.output_resolution, "3360x1890");
    assert_eq!(settings.stream.width, 3360);
    assert_eq!(settings.stream.height, 1890);
}

#[test]
fn load_or_detect_existing_file_ignores_detected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    // Write a settings file with custom resolution
    let mut settings = AppSettings::default();
    settings.video.base_resolution = "2560x1440".to_string();
    settings.save_to(&path).unwrap();
    // Should load from file, not use detected
    let loaded = AppSettings::load_or_detect(&path, Some((3360, 1890)));
    assert_eq!(loaded.video.base_resolution, "2560x1440");
}

#[test]
fn load_or_detect_no_detection_uses_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    // No file, no detection — should fall back to defaults
    let settings = AppSettings::load_or_detect(&path, None);
    assert_eq!(settings.video.base_resolution, "1920x1080");
}
```

- [ ] **Step 2: Verify tempfile is a dev-dependency**

Check `Cargo.toml` for `tempfile` in `[dev-dependencies]`. If missing:
```bash
cargo add tempfile --dev
```

- [ ] **Step 3: Run the tests**

Run: `cargo test load_or_detect -- --nocapture`
Expected: All 3 tests pass

- [ ] **Step 4: Run full test suite and clippy**

Run: `cargo test && cargo clippy`
Expected: All tests pass, no new warnings

- [ ] **Step 5: Commit**

```bash
git add src/settings.rs
git commit -m "test: add load_or_detect tests for first-launch resolution detection"
```
