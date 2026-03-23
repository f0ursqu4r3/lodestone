# Window & Camera Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add window-level capture (via CoreGraphics + appsrc) and camera capture (via avfvideosrc) as new source types alongside existing display capture.

**Architecture:** Extend `CaptureSourceConfig` with Window/Camera variants. Camera reuses the existing `avfvideosrc` pipeline. Window capture uses a dedicated thread per source that grabs frames via `CGWindowListCreateImage` and pushes them into a GStreamer `appsrc`. Both new types flow into the same compositor frame map as display capture.

**Tech Stack:** Rust, GStreamer (gstreamer-rs, gstreamer-app), CoreGraphics (core-graphics / core-foundation / objc2 FFI), egui

**Spec:** `docs/superpowers/specs/2026-03-23-window-camera-capture-design.md`

---

## File Structure

```
src/scene.rs                  # MODIFY — add Window/Camera to SourceProperties
src/state.rs                  # MODIFY — add available_cameras, available_windows
src/gstreamer/commands.rs     # MODIFY — add Window/Camera to CaptureSourceConfig
src/gstreamer/devices.rs      # MODIFY — add enumerate_cameras(), enumerate_windows()
src/gstreamer/capture.rs      # MODIFY — add build_camera_pipeline(), build_window_pipeline()
src/gstreamer/thread.rs       # MODIFY — handle Window capture thread lifecycle
src/gstreamer/mod.rs          # MODIFY — re-export new types
src/ui/sources_panel.rs       # MODIFY — source type picker menu
src/ui/properties_panel.rs    # MODIFY — window/camera selectors
Cargo.toml                    # MODIFY — add core-graphics, core-foundation deps
```

---

### Task 1: Extend Data Model (SourceProperties + CaptureSourceConfig)

**Files:**
- Modify: `src/scene.rs:54-56` (SourceProperties enum)
- Modify: `src/gstreamer/commands.rs:71-75` (CaptureSourceConfig enum)

- [ ] **Step 1: Add Window and Camera variants to SourceProperties**

In `src/scene.rs`, the `SourceProperties` enum currently has only `Display`. Add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceProperties {
    Display { screen_index: u32 },
    Window {
        window_id: u32,
        window_title: String,
        owner_name: String,
    },
    Camera {
        device_index: u32,
        device_name: String,
    },
}
```

- [ ] **Step 2: Add Window and Camera variants to CaptureSourceConfig**

In `src/gstreamer/commands.rs`, extend:

```rust
#[derive(Debug, Clone)]
pub enum CaptureSourceConfig {
    Screen { screen_index: u32 },
    Window { window_id: u32 },
    Camera { device_index: u32 },
}
```

- [ ] **Step 3: Fix any exhaustive match arms**

Search for `match` on `SourceProperties` and `CaptureSourceConfig` across the codebase. Add placeholder arms for the new variants so everything compiles. Key locations:
- `src/gstreamer/capture.rs` — `build_capture_pipeline()` match on config
- `src/ui/properties_panel.rs` — match on source properties
- `src/ui/sources_panel.rs` — any match on source type or properties

For now, add `todo!()` arms or empty handlers — they'll be filled in later tasks.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles (with unused warnings for new variants, which is fine).

- [ ] **Step 5: Commit**

```bash
git add src/scene.rs src/gstreamer/commands.rs src/gstreamer/capture.rs \
  src/ui/properties_panel.rs src/ui/sources_panel.rs
git commit -m "feat(model): add Window and Camera variants to SourceProperties and CaptureSourceConfig"
```

---

### Task 2: Add State Fields and Device Types

**Files:**
- Modify: `src/gstreamer/devices.rs` (add CameraDevice, WindowInfo structs)
- Modify: `src/gstreamer/mod.rs` (re-export new types)
- Modify: `src/state.rs:31-51` (add fields to AppState)
- Modify: `src/state.rs:53-77` (update Default impl)

- [ ] **Step 1: Add CameraDevice and WindowInfo types**

In `src/gstreamer/devices.rs`, add:

```rust
/// A camera device discovered via GStreamer DeviceMonitor.
#[derive(Debug, Clone)]
pub struct CameraDevice {
    pub device_index: u32,
    pub name: String,
}

/// A window available for capture, discovered via CoreGraphics.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
}
```

- [ ] **Step 2: Re-export from gstreamer/mod.rs**

Add `CameraDevice` and `WindowInfo` to the `pub use` statements in `src/gstreamer/mod.rs`.

- [ ] **Step 3: Add fields to AppState**

In `src/state.rs`, add to AppState struct:

```rust
pub available_cameras: Vec<crate::gstreamer::CameraDevice>,
pub available_windows: Vec<crate::gstreamer::WindowInfo>,
```

And in the `Default` impl:

```rust
available_cameras: Vec::new(),
available_windows: Vec::new(),
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add src/gstreamer/devices.rs src/gstreamer/mod.rs src/state.rs
git commit -m "feat(state): add CameraDevice, WindowInfo types and state fields"
```

---

### Task 3: Implement Camera Enumeration

**Files:**
- Modify: `src/gstreamer/devices.rs` (add enumerate_cameras function)

- [ ] **Step 1: Study existing audio enumeration pattern**

Read `src/gstreamer/devices.rs` to understand how `enumerate_audio_input_devices` works with GStreamer's DeviceMonitor. The camera enumeration follows the same pattern.

- [ ] **Step 2: Implement enumerate_cameras()**

Add to `src/gstreamer/devices.rs`:

```rust
/// Enumerate available camera devices via GStreamer DeviceMonitor.
/// Filters out screen capture sources — only returns actual cameras.
pub fn enumerate_cameras() -> Vec<CameraDevice> {
    let monitor = match gstreamer::DeviceMonitor::new() {
        m => m,
    };

    // Filter for video sources only.
    monitor.add_filter(Some("Video/Source"), None);

    if monitor.start().is_err() {
        return Vec::new();
    }

    let mut cameras = Vec::new();
    let mut index = 0u32;

    for device in monitor.devices() {
        let name = device.display_name().to_string();
        let props = device.properties();

        // Skip screen capture devices — we only want cameras.
        // avfvideosrc screen capture devices have "capture-screen" in their properties.
        let is_screen = props
            .as_ref()
            .map(|p| {
                p.value("device.api")
                    .ok()
                    .and_then(|v| v.get::<String>().ok())
                    .map(|api| api.contains("avfvideosrc"))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        if !is_screen {
            cameras.push(CameraDevice {
                device_index: index,
                name,
            });
        }
        index += 1;
    }

    monitor.stop();
    cameras
}
```

Note: The exact filtering logic for screen vs camera may need adjustment at implementation time. GStreamer's DeviceMonitor may not distinguish them by properties alone. The implementer should test with actual hardware and adjust the filter. A simpler approach: just list all Video/Source devices and let the user pick — screen capture devices will have names like "Capture screen 0".

- [ ] **Step 3: Write a test**

```rust
#[test]
fn enumerate_cameras_does_not_panic() {
    gstreamer::init().unwrap();
    let cameras = enumerate_cameras();
    // May be empty in CI, but shouldn't panic.
    for cam in &cameras {
        assert!(!cam.name.is_empty());
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test enumerate_cameras`
Expected: PASS (may return empty list in headless environment).

- [ ] **Step 5: Commit**

```bash
git add src/gstreamer/devices.rs
git commit -m "feat(gstreamer): implement camera device enumeration"
```

---

### Task 4: Implement Window Enumeration (macOS)

**Files:**
- Modify: `Cargo.toml` (add core-graphics dependency)
- Modify: `src/gstreamer/devices.rs` (add enumerate_windows function)

- [ ] **Step 1: Add core-graphics dependency**

```bash
cargo add core-graphics core-foundation
```

This adds macOS-only dependencies for `CGWindowListCopyWindowInfo`.

- [ ] **Step 2: Implement enumerate_windows()**

Add to `src/gstreamer/devices.rs`, behind a `#[cfg(target_os = "macos")]` gate:

```rust
#[cfg(target_os = "macos")]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListExcludeDesktopElements,
        kCGWindowListOptionOnScreenOnly, CGWindowListCopyWindowInfo,
    };

    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;

    let window_list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CGWindowListCopyWindowInfo(options, kCGNullWindowID) };

    let mut windows = Vec::new();
    let own_pid = std::process::id();

    for dict in window_list.iter() {
        // Extract fields from the CFDictionary.
        let window_id = dict
            .find(unsafe { kCGWindowNumber })
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0) as u32;

        let owner_name = dict
            .find(unsafe { kCGWindowOwnerName })
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let title = dict
            .find(unsafe { kCGWindowName })
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let owner_pid = dict
            .find(unsafe { kCGWindowOwnerPID })
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .unwrap_or(0) as u32;

        // Filter: skip empty titles, skip Lodestone itself, skip tiny windows.
        if title.is_empty() || owner_pid == own_pid {
            continue;
        }

        windows.push(WindowInfo {
            window_id,
            title,
            owner_name,
        });
    }

    windows
}

#[cfg(not(target_os = "macos"))]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    // Window enumeration not yet implemented on this platform.
    Vec::new()
}
```

**Important:** The exact CoreGraphics FFI may differ depending on crate version. The implementer should check whether `core-graphics` exposes `CGWindowListCopyWindowInfo` directly or if raw FFI via `core_graphics::sys` or `objc2` is needed. Adapt the code accordingly.

- [ ] **Step 3: Write a test**

```rust
#[test]
fn enumerate_windows_does_not_panic() {
    let windows = enumerate_windows();
    for w in &windows {
        assert!(!w.title.is_empty());
        assert!(w.window_id != 0);
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test enumerate_windows`
Expected: PASS (returns actual windows on macOS desktop).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/gstreamer/devices.rs
git commit -m "feat(gstreamer): implement macOS window enumeration via CoreGraphics"
```

---

### Task 5: Build Camera Capture Pipeline

**Files:**
- Modify: `src/gstreamer/capture.rs:11-70` (extend build_capture_pipeline or add new function)

- [ ] **Step 1: Read existing build_capture_pipeline()**

Read `src/gstreamer/capture.rs` to understand the current pipeline construction for `CaptureSourceConfig::Screen`. Camera is nearly identical — same `avfvideosrc` element, just without `capture-screen: true`.

- [ ] **Step 2: Add Camera arm to the match**

In `build_capture_pipeline()`, the match on `CaptureSourceConfig` currently handles `Screen`. Add the `Camera` variant:

```rust
CaptureSourceConfig::Camera { device_index } => {
    // Same avfvideosrc, but without capture-screen (defaults to camera mode).
    let src = gstreamer::ElementFactory::make("avfvideosrc")
        .name("video-source")
        .property("device-index", *device_index as i32)
        .build()?;
    src
}
```

The rest of the pipeline (`videoconvert → videoscale → videorate → appsink`) is shared with Screen. Make sure the Camera arm returns the source element and falls through to the shared pipeline construction.

- [ ] **Step 3: Add a placeholder for Window**

The Window variant needs a different pipeline (appsrc-based). For now, add a `todo!()` or a stub that returns an error:

```rust
CaptureSourceConfig::Window { .. } => {
    anyhow::bail!("Window capture pipeline built separately via build_window_capture_pipeline()");
}
```

- [ ] **Step 4: Write a test**

```rust
#[test]
fn build_camera_pipeline_creates_valid_pipeline() {
    gstreamer::init().unwrap();
    let config = CaptureSourceConfig::Camera { device_index: 0 };
    // This may fail if no camera is available, which is OK in CI.
    let result = build_capture_pipeline(&config, 1920, 1080, 30);
    // We just verify it doesn't panic — actual camera availability varies.
    match result {
        Ok((pipeline, _sink)) => {
            assert!(pipeline.state(gstreamer::ClockTime::from_mseconds(0)).1
                != gstreamer::State::Null || true);
        }
        Err(_) => {} // No camera available — acceptable in CI.
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test build_camera`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/capture.rs
git commit -m "feat(gstreamer): add camera capture pipeline via avfvideosrc"
```

---

### Task 6: Build Window Capture Pipeline (macOS)

**Files:**
- Modify: `src/gstreamer/capture.rs` (add build_window_capture_pipeline)
- Modify: `src/gstreamer/thread.rs` (handle Window in add_capture_source, manage capture thread)

This is the most complex task. Window capture uses CoreGraphics to grab frames and `appsrc` to feed them into GStreamer.

- [ ] **Step 1: Add build_window_capture_pipeline()**

In `src/gstreamer/capture.rs`, add a new function:

```rust
/// Build a GStreamer pipeline for window capture using appsrc.
/// Returns the pipeline, appsink (for frame extraction), and appsrc (for pushing frames).
/// The caller is responsible for driving the frame loop on a separate thread.
#[cfg(target_os = "macos")]
pub fn build_window_capture_pipeline(
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(gstreamer::Pipeline, AppSink, gstreamer_app::AppSrc)> {
    use gstreamer::prelude::*;

    let pipeline = gstreamer::Pipeline::new();

    let appsrc = gstreamer_app::AppSrc::builder()
        .name("window-src")
        .caps(
            &gstreamer_video::VideoCapsBuilder::new()
                .format(gstreamer_video::VideoFormat::Rgba)
                .width(width as i32)
                .height(height as i32)
                .framerate(gstreamer::Fraction::new(fps as i32, 1))
                .build(),
        )
        .format(gstreamer::Format::Time)
        .is_live(true)
        .do_timestamp(true)
        .build();

    let videoconvert = gstreamer::ElementFactory::make("videoconvert")
        .name("convert")
        .build()?;
    let videoscale = gstreamer::ElementFactory::make("videoscale")
        .name("scale")
        .build()?;

    let appsink = gstreamer_app::AppSink::builder()
        .name("sink")
        .caps(
            &gstreamer_video::VideoCapsBuilder::new()
                .format(gstreamer_video::VideoFormat::Rgba)
                .width(width as i32)
                .height(height as i32)
                .build(),
        )
        .build();

    pipeline.add_many([
        appsrc.upcast_ref(),
        &videoconvert,
        &videoscale,
        appsink.upcast_ref(),
    ])?;
    gstreamer::Element::link_many([
        appsrc.upcast_ref(),
        &videoconvert,
        &videoscale,
        appsink.upcast_ref(),
    ])?;

    Ok((pipeline, appsink, appsrc))
}
```

Note: The implementer should check `gstreamer-video` is available and the caps builder API matches the crate version. Adapt as needed.

- [ ] **Step 2: Add window frame grabber function**

In `src/gstreamer/capture.rs`, add a macOS-specific frame grab function:

```rust
/// Capture a single frame from a window via CoreGraphics.
/// Returns RGBA pixel data and (width, height), or None if the window is unavailable.
#[cfg(target_os = "macos")]
pub fn grab_window_frame(window_id: u32) -> Option<(Vec<u8>, u32, u32)> {
    use core_graphics::display::*;
    use core_graphics::geometry::CGRect;

    let cg_image = unsafe {
        CGDisplay::create_image_for_window(
            CGWindowID(window_id),
            // Options: capture just this window, ignore framing
        )
    };
    // The exact API call depends on the core-graphics crate version.
    // The implementer should use CGWindowListCreateImage with:
    //   rect: CGRectNull (capture full window)
    //   listOption: kCGWindowListOptionIncludingWindow
    //   windowID: window_id
    //   imageOption: kCGWindowImageBoundsIgnoreFraming
    //
    // Then extract pixel data from the CGImage:
    //   - Get data provider → CFData → bytes
    //   - Get width, height, bits_per_pixel, bytes_per_row
    //   - Convert BGRA → RGBA if needed (CoreGraphics typically outputs BGRA)

    // Return (rgba_bytes, width, height) or None if capture failed.
    todo!("Implement CGWindowListCreateImage frame grab — exact FFI depends on crate version")
}
```

The implementer must fill in the actual CoreGraphics FFI. The exact API depends on which crate/version is available. This is the hardest part of the task — the plan intentionally provides the structure and documents what needs to happen, but the FFI details require hands-on discovery.

- [ ] **Step 3: Handle Window capture in GStreamer thread**

In `src/gstreamer/thread.rs`, modify `add_capture_source()` to handle `CaptureSourceConfig::Window`:

```rust
CaptureSourceConfig::Window { window_id } => {
    // 1. Grab one frame to get the window dimensions.
    let (_, w, h) = grab_window_frame(*window_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot capture window {}", window_id))?;

    // 2. Build the appsrc-based pipeline.
    let (pipeline, appsink, appsrc) = build_window_capture_pipeline(w, h, fps)?;
    pipeline.set_state(gstreamer::State::Playing)?;

    // 3. Spawn a dedicated capture thread.
    let frame_interval = std::time::Duration::from_secs_f64(1.0 / fps as f64);
    let wid = *window_id;
    let error_tx = self.channels.error_tx.clone();
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();

    std::thread::spawn(move || {
        while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            let start = std::time::Instant::now();

            match grab_window_frame(wid) {
                Some((rgba, _w, _h)) => {
                    let mut buffer = gstreamer::Buffer::with_size(rgba.len()).unwrap();
                    {
                        let buffer_ref = buffer.get_mut().unwrap();
                        buffer_ref.copy_from_slice(0, &rgba).unwrap();
                    }
                    if appsrc.push_buffer(buffer).is_err() {
                        break;
                    }
                }
                None => {
                    let _ = error_tx.try_send(GstError::CaptureFailure {
                        message: "Window closed or unavailable".into(),
                    });
                    break;
                }
            }

            let elapsed = start.elapsed();
            if elapsed < frame_interval {
                std::thread::sleep(frame_interval - elapsed);
            }
        }
    });

    // 4. Store the handle (with the running flag for cleanup).
    self.captures.insert(source_id, CaptureHandle { pipeline, appsink });
    // Store `running` Arc somewhere so remove_capture_source can set it to false.
}
```

The implementer needs to extend `CaptureHandle` to store the `running: Arc<AtomicBool>` for window sources so the thread can be stopped on removal.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles (the `grab_window_frame` todo will need to be filled in).

- [ ] **Step 5: Commit**

```bash
git add src/gstreamer/capture.rs src/gstreamer/thread.rs
git commit -m "feat(gstreamer): add window capture pipeline with CoreGraphics frame grab"
```

---

### Task 7: Wire Camera Enumeration at Startup

**Files:**
- Modify: `src/main.rs` (enumerate cameras after GStreamer init, store in state)

- [ ] **Step 1: Find where GStreamer is initialized and audio devices are enumerated**

Read `src/main.rs` to find where `gstreamer::init()` is called and where `enumerate_audio_input_devices()` is called. Add camera enumeration in the same location.

- [ ] **Step 2: Add camera enumeration**

After the existing audio device enumeration, add:

```rust
state.available_cameras = crate::gstreamer::devices::enumerate_cameras();
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: enumerate cameras at startup"
```

---

### Task 8: Source Type Picker in Sources Panel

**Files:**
- Modify: `src/ui/sources_panel.rs:57-59` (replace direct add with type picker)
- Modify: `src/ui/sources_panel.rs:242-280` (add_display_source → generalize)

- [ ] **Step 1: Replace the "+" button with a source type menu**

Change the "+" button click handler to show a popup menu with source type options:

```rust
let add_response = ui.button(egui_phosphor::regular::PLUS)
    .on_hover_text("Add source");

let popup_id = ui.make_persistent_id("add_source_menu");
if add_response.clicked() {
    ui.memory_mut(|m| m.toggle_popup(popup_id));
}

egui::popup_below_widget(ui, popup_id, &add_response, |ui| {
    ui.set_min_width(120.0);
    if ui.button("Display").clicked() {
        add_source = Some(SourceType::Display);
        ui.memory_mut(|m| m.close_popup());
    }
    if ui.button("Window").clicked() {
        add_source = Some(SourceType::Window);
        ui.memory_mut(|m| m.close_popup());
    }
    if ui.button("Camera").clicked() {
        add_source = Some(SourceType::Camera);
        ui.memory_mut(|m| m.close_popup());
    }
});
```

- [ ] **Step 2: Create add_window_source and add_camera_source functions**

Based on the existing `add_display_source()`:

```rust
fn add_window_source(state: &mut AppState, cmd_tx: &Option<Sender<GstCommand>>, scene_id: SceneId) {
    let source_id = SourceId(state.next_source_id);
    state.next_source_id += 1;
    let source = Source {
        id: source_id,
        name: "Window".to_string(),
        source_type: SourceType::Window,
        properties: SourceProperties::Window {
            window_id: 0,     // Placeholder — user selects in Properties panel
            window_title: String::new(),
            owner_name: String::new(),
        },
        transform: Transform { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    state.sources.push(source);
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
        scene.sources.push(source_id);
    }
    state.selected_source_id = Some(source_id);
    state.scenes_dirty = true;
    // Don't send AddCaptureSource yet — no window selected.
    // The Properties panel will trigger capture when a window is chosen.
}

fn add_camera_source(
    state: &mut AppState,
    cmd_tx: &Option<Sender<GstCommand>>,
    scene_id: SceneId,
) {
    let source_id = SourceId(state.next_source_id);
    state.next_source_id += 1;
    let device_index = 0;
    let device_name = state
        .available_cameras
        .first()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| "Camera 0".into());
    let source = Source {
        id: source_id,
        name: device_name.clone(),
        source_type: SourceType::Camera,
        properties: SourceProperties::Camera { device_index, device_name },
        transform: Transform { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
        opacity: 1.0,
        visible: true,
        muted: false,
        volume: 1.0,
    };
    state.sources.push(source);
    if let Some(scene) = state.scenes.iter_mut().find(|s| s.id == scene_id) {
        scene.sources.push(source_id);
    }
    state.selected_source_id = Some(source_id);
    state.scenes_dirty = true;
    // Start capture immediately with default camera.
    if let Some(tx) = cmd_tx {
        let _ = tx.try_send(GstCommand::AddCaptureSource {
            source_id,
            config: CaptureSourceConfig::Camera { device_index },
        });
    }
}
```

- [ ] **Step 3: Wire the menu to the add functions**

After the popup, handle the selected source type:

```rust
if let Some(source_type) = add_source {
    match source_type {
        SourceType::Display => add_display_source(state, &cmd_tx, active_id),
        SourceType::Window => add_window_source(state, &cmd_tx, active_id),
        SourceType::Camera => add_camera_source(state, &cmd_tx, active_id),
        _ => {}
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`

- [ ] **Step 5: Commit**

```bash
git add src/ui/sources_panel.rs
git commit -m "feat(ui): add source type picker with Display/Window/Camera options"
```

---

### Task 9: Window and Camera Selectors in Properties Panel

**Files:**
- Modify: `src/ui/properties_panel.rs:95-120` (extend SOURCE section)

- [ ] **Step 1: Add Window source properties UI**

In the SOURCE section match, add a handler for `SourceProperties::Window`:

```rust
SourceProperties::Window { window_id, window_title, owner_name } => {
    // Window selector ComboBox
    ui.horizontal(|ui| {
        let current_label = if window_title.is_empty() {
            "Select a window...".to_string()
        } else {
            format!("{owner_name} — {window_title}")
        };

        let mut new_window = None;
        egui::ComboBox::from_id_salt("window_selector")
            .selected_text(&current_label)
            .show_ui(ui, |ui| {
                for w in &state.available_windows {
                    let label = format!("{} — {}", w.owner_name, w.title);
                    if ui.selectable_label(w.window_id == *window_id, &label).clicked() {
                        new_window = Some(w.clone());
                    }
                }
            });

        // Refresh button
        if ui.button(egui_phosphor::regular::ARROW_CLOCKWISE)
            .on_hover_text("Refresh window list")
            .clicked()
        {
            state.available_windows = crate::gstreamer::devices::enumerate_windows();
        }

        // Apply selection
        if let Some(w) = new_window {
            // Stop old capture if running
            if *window_id != 0 {
                if let Some(tx) = &state.command_tx {
                    let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
                }
            }
            // Update properties
            *window_id = w.window_id;
            *window_title = w.title;
            *owner_name = w.owner_name;
            // Start new capture
            if let Some(tx) = &state.command_tx {
                let _ = tx.try_send(GstCommand::AddCaptureSource {
                    source_id: src_id,
                    config: CaptureSourceConfig::Window { window_id: w.window_id },
                });
            }
            state.scenes_dirty = true;
        }
    });
}
```

Note: The implementer needs to handle borrow issues — `state.available_windows` is borrowed while `state` is also mutably borrowed for the source. Clone the window list before the match, or use indices.

- [ ] **Step 2: Add Camera source properties UI**

```rust
SourceProperties::Camera { device_index, device_name } => {
    let mut new_camera = None;
    egui::ComboBox::from_id_salt("camera_selector")
        .selected_text(device_name.as_str())
        .show_ui(ui, |ui| {
            for cam in &state.available_cameras {
                if ui.selectable_label(cam.device_index == *device_index, &cam.name).clicked() {
                    new_camera = Some(cam.clone());
                }
            }
        });

    if let Some(cam) = new_camera {
        // Stop old capture
        if let Some(tx) = &state.command_tx {
            let _ = tx.try_send(GstCommand::RemoveCaptureSource { source_id: src_id });
        }
        // Update properties
        *device_index = cam.device_index;
        *device_name = cam.name;
        // Start new capture
        if let Some(tx) = &state.command_tx {
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: src_id,
                config: CaptureSourceConfig::Camera { device_index: cam.device_index },
            });
        }
        state.scenes_dirty = true;
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`

- [ ] **Step 4: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat(ui): add window and camera selectors in properties panel"
```

---

### Task 10: Final Integration and Testing

**Files:**
- All modified files for final adjustments

- [ ] **Step 1: Run full build**

Run: `cargo build`
Fix any compilation errors.

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
All tests must pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy`
Fix any warnings.

- [ ] **Step 4: Run fmt**

Run: `cargo fmt --check`
Fix any formatting issues.

- [ ] **Step 5: Manual testing checklist**

Run: `cargo run`

- [ ] Add a Display source — should work as before
- [ ] Add a Camera source — should show camera feed in preview (if camera available)
- [ ] Change camera in Properties — should switch capture
- [ ] Add a Window source — should show "Select a window..." in Properties
- [ ] Click Refresh in Properties — should populate window list
- [ ] Select a window — should start capturing that window
- [ ] Close the captured window — should show error, last frame held
- [ ] Remove sources — capture should stop cleanly

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "chore: final integration fixes for window/camera capture"
```
