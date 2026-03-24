# Exclude Lodestone Windows from Display Capture — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `avfvideosrc`-based display capture with ScreenCaptureKit so Lodestone's own windows can be excluded from recordings/streams.

**Architecture:** Add a ScreenCaptureKit FFI module that captures display frames (excluding own PID's windows) and feeds them via channel to an `appsrc`-based GStreamer pipeline — the same pattern as existing window capture. A new setting controls the exclusion. All call sites that construct `CaptureSourceConfig::Screen` pass the setting value.

**Tech Stack:** Rust, macOS ScreenCaptureKit (via `objc2` crates or raw FFI), GStreamer (`appsrc`), `anyhow`

---

## File Structure

### New files:
- `src/gstreamer/screencapturekit.rs` — ScreenCaptureKit FFI: start/stop display capture, frame channel

### Modified files:
- `Cargo.toml` — add ScreenCaptureKit/ObjC dependencies
- `src/settings.rs` — add `exclude_self_from_capture` to `GeneralSettings`
- `src/ui/settings/general.rs` — add toggle UI
- `src/gstreamer/commands.rs` — add `exclude_self: bool` to `CaptureSourceConfig::Screen`
- `src/gstreamer/mod.rs` — add `screencapturekit` module
- `src/gstreamer/capture.rs` — new `build_display_capture_pipeline()` using `appsrc`
- `src/gstreamer/thread.rs` — new `add_display_capture_source()` method, update `CaptureHandle`, update `remove_capture_source()`
- `src/main.rs` — pass `exclude_self` when constructing `Screen` config
- `src/ui/scenes_panel.rs` — pass `exclude_self` in `apply_scene_diff()` and `send_capture_for_scene()`
- `src/ui/sources_panel.rs` — pass `exclude_self` in `start_capture_from_properties()`
- `src/ui/preview_panel.rs` — pass `exclude_self` in capture config construction

---

## Task 1: Add setting and update CaptureSourceConfig

**Files:**
- Modify: `src/settings.rs`
- Modify: `src/ui/settings/general.rs`
- Modify: `src/gstreamer/commands.rs`

This task adds the plumbing without changing capture behavior.

- [ ] **Step 1: Add setting to GeneralSettings**

In `src/settings.rs`, add `exclude_self_from_capture: bool` to `GeneralSettings` struct (after `snap_grid_size`):

```rust
/// Exclude Lodestone windows from display capture.
pub exclude_self_from_capture: bool,
```

And in the `Default` impl, add:
```rust
exclude_self_from_capture: true,
```

- [ ] **Step 2: Add toggle to settings UI**

In `src/ui/settings/general.rs`, add a new CAPTURE section after the existing sections. Use the existing `draw_toggle` pattern:

```rust
changed |= super::draw_toggle(ui, "Exclude Lodestone from capture", &mut settings.exclude_self_from_capture);
```

- [ ] **Step 3: Add exclude_self to CaptureSourceConfig::Screen**

In `src/gstreamer/commands.rs`, update the `Screen` variant:

```rust
Screen { screen_index: u32, exclude_self: bool },
```

- [ ] **Step 4: Fix all compilation errors from the new field**

Every place that constructs or pattern-matches `CaptureSourceConfig::Screen` now needs `exclude_self`. For now, pass `false` at all construction sites and use `..` or destructure the new field at match sites. The call sites are:

**Construction sites** (add `exclude_self: false`):
- `src/main.rs` ~line 457: startup scene capture loop
- `src/ui/scenes_panel.rs`: `apply_scene_diff()` ~line 397 and `send_capture_for_scene()` ~line 487
- `src/ui/sources_panel.rs`: `start_capture_from_properties()` ~line 596
- `src/ui/preview_panel.rs`: ~line 217
- `src/gstreamer/capture.rs`: test `build_capture_pipeline_creates_valid_pipeline` ~line 286
- `src/gstreamer/commands.rs`: tests ~lines 244, 247
- `src/gstreamer/thread.rs`: test ~line 810

**Match/destructure sites** (add `exclude_self: _` or use `..`):
- `src/gstreamer/thread.rs` in `add_capture_source()` — destructure the new field (ignore for now)
- `src/gstreamer/capture.rs` in `build_capture_pipeline()` Screen match arm ~line 21

- [ ] **Step 5: Build and test**

Run: `cargo build && cargo test`
Expected: Clean build, 125 tests pass. No behavior change.

- [ ] **Step 6: Commit**

```bash
git add src/settings.rs src/ui/settings/general.rs src/gstreamer/commands.rs src/gstreamer/capture.rs src/gstreamer/thread.rs src/main.rs src/ui/scenes_panel.rs src/ui/sources_panel.rs src/ui/preview_panel.rs
git commit -m "feat: add exclude_self_from_capture setting and CaptureSourceConfig field"
```

---

## Task 2: Wire setting value to all call sites

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/scenes_panel.rs`
- Modify: `src/ui/sources_panel.rs`
- Modify: `src/ui/preview_panel.rs`

Replace the `exclude_self: false` placeholders with the actual setting value.

- [ ] **Step 1: Update all call sites**

Each call site has access to `state: &AppState` (or `&mut AppState`). Replace `exclude_self: false` with:

```rust
exclude_self: state.settings.general.exclude_self_from_capture
```

At all 5 locations identified in Task 1 Step 4. The `main.rs` startup path also has access to the loaded settings.

- [ ] **Step 2: Build and test**

Run: `cargo build && cargo test`
Expected: Clean build. No behavior change yet (the field is read but not acted on).

- [ ] **Step 3: Commit**

```bash
git add src/main.rs src/ui/scenes_panel.rs src/ui/sources_panel.rs src/ui/preview_panel.rs
git commit -m "feat: wire exclude_self_from_capture setting to all capture call sites"
```

---

## Task 3: ScreenCaptureKit FFI module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/gstreamer/screencapturekit.rs`
- Modify: `src/gstreamer/mod.rs`

This is the core new code. It wraps macOS ScreenCaptureKit to capture display frames with optional PID-based window exclusion.

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add under `[dependencies]`:

```toml
objc2 = "0.6"
objc2-foundation = { version = "0.3", features = ["NSArray", "NSError", "NSString", "NSThread"] }
objc2-screen-capture-kit = { version = "0.3", features = ["SCContentFilter", "SCContentSharingPicker", "SCDisplay", "SCRunningApplication", "SCShareableContent", "SCStream", "SCStreamConfiguration", "SCStreamOutput", "SCWindow"] }
objc2-core-media = { version = "0.3", features = ["CMSampleBuffer", "CMBlockBuffer", "CMFormatDescription"] }
objc2-core-video = { version = "0.3", features = ["CVBuffer", "CVImageBuffer", "CVPixelBuffer", "CVPixelBufferPool"] }
```

Note: The exact feature flags may need adjustment during implementation. The `objc2` ecosystem uses fine-grained features. Start with the above and add any missing features the compiler requests.

Also add macOS framework links if needed. The `objc2-screen-capture-kit` crate should handle this automatically via its build script.

- [ ] **Step 2: Create `src/gstreamer/screencapturekit.rs`**

Implement the module with this public API:

```rust
use crate::gstreamer::types::RgbaFrame;
use anyhow::Result;
use std::sync::mpsc;

/// Opaque handle to a running SCStream capture.
pub struct SCStreamHandle { /* ... */ }

/// Start capturing a display via ScreenCaptureKit.
///
/// - `screen_index`: index into SCShareableContent.displays
/// - `width`, `height`: output resolution (SCK scales to this)
/// - `fps`: target frame rate
/// - `exclude_own_pid`: if true, exclude all windows from this process
///
/// Returns a handle (for stopping) and a receiver for RGBA frames.
pub fn start_display_capture(
    screen_index: u32,
    width: u32,
    height: u32,
    fps: u32,
    exclude_own_pid: bool,
) -> Result<(SCStreamHandle, mpsc::Receiver<RgbaFrame>)> {
    // 1. Get SCShareableContent (blocks on async completion handler)
    // 2. Find the SCDisplay at screen_index
    // 3. Build SCContentFilter:
    //    - If exclude_own_pid: filter with desktopIndependentWindows
    //      excluding windows where owningApplication.processID == our PID
    //    - Else: filter for display with no exclusions
    // 4. Configure SCStreamConfiguration:
    //    - width, height, pixelFormat (kCVPixelFormatType_32BGRA)
    //    - minimumFrameInterval (CMTime from fps)
    //    - showsCursor = true (parity with avfvideosrc)
    // 5. Create SCStream with filter + config
    // 6. Set up SCStreamOutput delegate that:
    //    - Receives CMSampleBuffer in stream:didOutputSampleBuffer:ofType:
    //    - Extracts CVPixelBuffer → lock base address → copy RGBA bytes
    //    - Converts BGRA → RGBA (swap B/R channels, same as grab_window_frame)
    //    - Sends RgbaFrame { width, height, data } over channel
    // 7. Start capture (startCapture completionHandler)
    // 8. Return (handle, receiver)
}

/// Stop a running display capture.
pub fn stop_display_capture(handle: SCStreamHandle) -> Result<()> {
    // Call handle.stream.stopCapture()
}
```

**Implementation notes:**
- ScreenCaptureKit APIs are async (completion handlers). Use `std::sync::mpsc::channel` to bridge from ObjC callbacks to Rust.
- The SCStreamOutput delegate must be an ObjC object. With `objc2`, implement the `SCStreamOutput` protocol on a custom class that holds a `Sender<RgbaFrame>`.
- BGRA → RGBA conversion: swap bytes [0] and [2] for each pixel, same as `grab_window_frame` in `capture.rs:124-126`.
- Permission denied: `SCShareableContent` returns an `NSError` if screen recording is denied. Convert to `anyhow::Error` with a descriptive message.
- `SCStreamHandle` should own the `SCStream` and any allocated ObjC objects to prevent premature deallocation.

- [ ] **Step 3: Register module in `src/gstreamer/mod.rs`**

Add:
```rust
#[cfg(target_os = "macos")]
pub mod screencapturekit;
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: Compiles. The module has no callers yet.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/gstreamer/screencapturekit.rs src/gstreamer/mod.rs
git commit -m "feat: add ScreenCaptureKit FFI module for display capture"
```

---

## Task 4: Display capture pipeline and thread integration

**Files:**
- Modify: `src/gstreamer/capture.rs`
- Modify: `src/gstreamer/thread.rs`

Replace `avfvideosrc` display capture with SCK-backed `appsrc` pipeline.

- [ ] **Step 1: Add `build_display_capture_pipeline()` to capture.rs**

Create a new function (similar to `build_window_capture_pipeline`):

```rust
/// Build a GStreamer pipeline for display capture fed by ScreenCaptureKit.
/// Returns (pipeline, appsink, appsrc).
#[cfg(target_os = "macos")]
pub fn build_display_capture_pipeline(
    width: u32,
    height: u32,
    fps: u32,
) -> anyhow::Result<(gstreamer::Pipeline, AppSink, AppSrc)> {
    // Same structure as build_window_capture_pipeline:
    // appsrc (live, do-timestamp=true, RGBA caps at width x height)
    //   → videoconvert → videoscale → appsink
    // No videorate (SCK controls frame timing).
}
```

- [ ] **Step 2: Update CaptureHandle in thread.rs**

Add `sck_handle` field:

```rust
struct CaptureHandle {
    pipeline: gstreamer::Pipeline,
    appsink: AppSink,
    capture_running: Option<Arc<AtomicBool>>,
    #[cfg(target_os = "macos")]
    sck_handle: Option<super::screencapturekit::SCStreamHandle>,
}
```

Update any place that constructs `CaptureHandle` to include `sck_handle: None`.

- [ ] **Step 3: Add `add_display_capture_source()` to GstThread**

New method on GstThread, modeled on `add_window_capture_source()`:

```rust
#[cfg(target_os = "macos")]
fn add_display_capture_source(
    &mut self,
    source_id: SourceId,
    screen_index: u32,
    exclude_self: bool,
) {
    // 1. Start SCK capture:
    //    let (sck_handle, frame_rx) = screencapturekit::start_display_capture(
    //        screen_index, width, height, fps, exclude_self
    //    )?;
    //    Use reasonable defaults for width/height/fps (e.g., 1920x1080 @ 30)
    //    or derive from the display's native resolution if available.
    //
    // 2. Build display capture pipeline:
    //    let (pipeline, appsink, appsrc) = build_display_capture_pipeline(w, h, fps)?;
    //    pipeline.set_state(Playing)?;
    //
    // 3. Spawn frame-pump thread:
    //    let running = Arc::new(AtomicBool::new(true));
    //    std::thread::Builder::new().name("display-capture-{screen_index}").spawn(move || {
    //        while running.load(Ordering::Relaxed) {
    //            match frame_rx.recv_timeout(Duration::from_millis(100)) {
    //                Ok(frame) => {
    //                    // Create GStreamer buffer from frame.data
    //                    // Set caps if dimensions changed
    //                    // appsrc.push_buffer(buffer)
    //                }
    //                Err(RecvTimeoutError::Timeout) => continue,
    //                Err(RecvTimeoutError::Disconnected) => break,
    //            }
    //        }
    //    });
    //
    // 4. Store handle:
    //    self.captures.insert(source_id, CaptureHandle {
    //        pipeline, appsink, capture_running: Some(running),
    //        sck_handle: Some(sck_handle),
    //    });
}
```

- [ ] **Step 4: Update `add_capture_source()` to use new display path**

In `add_capture_source()`, change the `Screen` arm:

```rust
CaptureSourceConfig::Screen { screen_index, exclude_self } => {
    #[cfg(target_os = "macos")]
    self.add_display_capture_source(source_id, screen_index, exclude_self);
}
```

Remove the old `avfvideosrc` path for Screen.

- [ ] **Step 5: Update `remove_capture_source()` to stop SCK**

Before setting pipeline to Null, stop the SCK stream:

```rust
fn remove_capture_source(&mut self, source_id: SourceId) {
    if let Some(handle) = self.captures.remove(&source_id) {
        if let Some(running) = &handle.capture_running {
            running.store(false, Ordering::Relaxed);
        }
        #[cfg(target_os = "macos")]
        if let Some(sck) = handle.sck_handle {
            let _ = super::screencapturekit::stop_display_capture(sck);
        }
        let _ = handle.pipeline.set_state(gstreamer::State::Null);
    }
}
```

- [ ] **Step 6: Send error on capture failure**

If `start_display_capture()` fails (permission denied, display not found), send a `GstError::CaptureFailure` through the error channel:

```rust
if let Err(e) = result {
    let _ = self.channels.error_tx.send(GstError::CaptureFailure { message: format!(
        "Display capture failed: {}. Check screen recording permission.", e
    ) });
}
```

- [ ] **Step 7: Build and test**

Run: `cargo build && cargo test`
Expected: Compiles. Tests pass (GStreamer tests don't exercise display capture).

- [ ] **Step 8: Manual test**

Run: `cargo run`
- Add a Display source
- Verify video appears in preview
- Open Lodestone settings window — verify it does NOT appear in capture
- Toggle the setting off in General settings, re-add the display source — verify Lodestone windows now appear

- [ ] **Step 9: Commit**

```bash
git add src/gstreamer/capture.rs src/gstreamer/thread.rs
git commit -m "feat: replace avfvideosrc with ScreenCaptureKit for display capture"
```

---

## Verification

After all tasks:

- [ ] **Full build**: `cargo build`
- [ ] **Clippy clean**: `cargo clippy`
- [ ] **Tests pass**: `cargo test`
- [ ] **Format check**: `cargo fmt --check`
- [ ] **Manual test**: Display capture works with exclusion on and off
- [ ] **Permission test**: Revoke screen recording permission, verify error message appears (not a crash)
