# Dynamic Window Source Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace static window-ID-based capture with application-tracking dynamic window source using ScreenCaptureKit, including "Any Fullscreen Application" mode.

**Architecture:** Update the data model (`SourceProperties::Window`) to track by bundle ID instead of window ID. Add SCK-based window capture functions. Build a `WindowWatcher` that runs on the GStreamer thread, periodically resolving the best window for each active window source and live-swapping the SCK filter. Remove the old CoreGraphics polling path.

**Tech Stack:** Rust, ScreenCaptureKit (via objc2-screen-capture-kit), GStreamer (appsrc pipeline), egui (properties UI)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/scene.rs` | Modify | Update `SourceProperties::Window` fields, add `WindowCaptureMode` enum |
| `src/gstreamer/devices.rs` | Modify | Enhanced `WindowInfo` with bundle_id/bounds, new `AppInfo` struct, `enumerate_applications()` |
| `src/gstreamer/screencapturekit.rs` | Modify | Add `start_window_capture()`, `update_window_target()`, `CaptureKind` enum, update `SCStreamHandle` |
| `src/gstreamer/window_watcher.rs` | Create | `WindowWatcher` struct with periodic resolution logic and fullscreen detection |
| `src/gstreamer/thread.rs` | Modify | Replace `add_window_capture_source()` with SCK-based version, integrate `WindowWatcher` into poll loop |
| `src/gstreamer/capture.rs` | Modify | Remove `grab_window_frame()` and `build_window_capture_pipeline()` |
| `src/gstreamer/commands.rs` | Modify | Update `CaptureSourceConfig::Window` fields |
| `src/ui/properties_panel.rs` | Modify | Replace window dropdown with app selector + mode toggle + pin toggle |
| `src/ui/sources_panel.rs` | Modify | Update `start_capture_from_properties()` for new Window variant |
| `src/ui/scenes_panel.rs` | Modify | Update Window pattern matches at lines 446 and 564 |
| `src/ui/library_panel.rs` | Modify | Update default Window source creation at line 296 |
| `src/ui/preview_panel.rs` | Modify | Update Window pattern match at line 841 |
| `src/state.rs` | Modify | Replace `available_windows` with `available_apps`, add window status tracking |
| `src/main.rs` | Modify | Update startup enumeration and Window pattern match at line 571 |
| `src/gstreamer/mod.rs` | Modify | Add `pub mod window_watcher;`, update re-exports for `AppInfo` |
| `Cargo.toml` | Modify | Add required `objc2-screen-capture-kit` features for window filtering |

---

### Task 1: Update Data Model (`scene.rs`)

**Files:**
- Modify: `src/scene.rs:152-162` (SourceProperties::Window variant)

- [ ] **Step 1: Add `WindowCaptureMode` enum**

Add before `SourceProperties`:

```rust
/// How the window source selects its capture target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowCaptureMode {
    /// Track a specific application by bundle ID.
    Application {
        bundle_id: String,
        app_name: String,
        /// Pin to a specific window by title substring (None = track frontmost).
        pinned_title: Option<String>,
    },
    /// Automatically capture whatever application is fullscreen.
    AnyFullscreen,
}
```

- [ ] **Step 2: Update `SourceProperties::Window`**

Replace the existing Window variant:

```rust
Window {
    mode: WindowCaptureMode,
    /// Runtime-only: the currently tracked window ID. Resolved by WindowWatcher.
    #[serde(skip)]
    current_window_id: Option<u32>,
},
```

- [ ] **Step 3: Update `Default` impl for `SourceProperties`**

Find the Default impl and update the Window default to use `AnyFullscreen`:

```rust
// In the Default impl or wherever Window defaults are created
SourceProperties::Window {
    mode: WindowCaptureMode::AnyFullscreen,
    current_window_id: None,
}
```

- [ ] **Step 4: Fix all compilation errors from the variant change**

The old variant had `{ window_id, window_title, owner_name }`. Every file that pattern-matches against `SourceProperties::Window` must be updated. Here is the complete list:

1. `src/ui/properties_panel.rs:574` — destructuring in draw_source_properties (fixed in Task 8)
2. `src/ui/sources_panel.rs:682` — pattern match in start_capture_from_properties (fixed in Task 9)
3. `src/ui/scenes_panel.rs:446` — Window arm sends AddCaptureSource with old `window_id`
4. `src/ui/scenes_panel.rs:564` — same pattern, second occurrence
5. `src/ui/library_panel.rs:296` — default Window source construction
6. `src/ui/preview_panel.rs:841` — Window arm with `window_id != 0` guard
7. `src/main.rs:571` — Window arm in startup capture restoration
8. `src/scene.rs:494` — migration check (just pattern match, should work with `..`)

For files fixed in later tasks (properties_panel, sources_panel), temporarily stub the pattern to `SourceProperties::Window { .. } => {}` so the build passes. For the others, update now:

**scenes_panel.rs:446 and :564** — both occurrences:
```rust
crate::scene::SourceProperties::Window { ref mode, .. } => {
    let _ = tx.try_send(GstCommand::AddCaptureSource {
        source_id: src_id,
        config: CaptureSourceConfig::Window {
            mode: mode.clone(),
        },
    });
}
```

**library_panel.rs:296:**
```rust
SourceProperties::Window {
    mode: crate::scene::WindowCaptureMode::AnyFullscreen,
    current_window_id: None,
}
```

**preview_panel.rs:841:**
```rust
crate::scene::SourceProperties::Window { ref mode, .. } => {
    let _ = cmd_tx.try_send(crate::gstreamer::GstCommand::AddCaptureSource {
        source_id: src_id,
        config: crate::gstreamer::CaptureSourceConfig::Window {
            mode: mode.clone(),
        },
    });
    state.capture_active = true;
}
```

**main.rs:571:**
```rust
crate::scene::SourceProperties::Window { ref mode, .. } => {
    if let Some(ref tx) = state.command_tx {
        let _ = tx.try_send(gstreamer::GstCommand::AddCaptureSource {
            source_id: src_id,
            config: gstreamer::CaptureSourceConfig::Window {
                mode: mode.clone(),
            },
        });
    }
}
```

Run: `cargo check 2>&1 | head -60`
Expected: Build passes (with possible warnings about unused imports)

- [ ] **Step 5: Commit**

```bash
git add src/scene.rs src/ui/scenes_panel.rs src/ui/library_panel.rs src/ui/preview_panel.rs src/main.rs
git commit -m "refactor: update Window source to track by application bundle ID"
```

---

### Task 2: Enhanced Window Enumeration (`devices.rs`)

**Files:**
- Modify: `src/gstreamer/devices.rs:16-146`

- [ ] **Step 1: Update `WindowInfo` struct**

Replace the existing struct:

```rust
/// A window available for capture, discovered via ScreenCaptureKit.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: u32,
    pub title: String,
    pub owner_name: String,
    pub bundle_id: String,
    pub bounds: (f64, f64, f64, f64), // x, y, width, height
    pub is_on_screen: bool,
    pub is_fullscreen: bool,
}
```

- [ ] **Step 2: Add `AppInfo` struct**

```rust
/// An application with one or more capturable windows.
#[derive(Debug, Clone)]
pub struct AppInfo {
    pub bundle_id: String,
    pub name: String,
    pub windows: Vec<WindowInfo>,
}
```

- [ ] **Step 3: Rewrite `enumerate_windows()` to use ScreenCaptureKit**

Replace the CoreGraphics-based implementation. This requires `get_shareable_content()` in `screencapturekit.rs` to be made `pub(crate)` (done in Task 3, Step 3 — but do it now to unblock this step).

```rust
#[cfg(target_os = "macos")]
pub fn enumerate_windows() -> Vec<WindowInfo> {
    use objc2_screen_capture_kit::{SCDisplay, SCWindow};
    use objc2_foundation::NSArray;

    let content = match super::screencapturekit::get_shareable_content() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to get shareable content: {e}");
            return Vec::new();
        }
    };

    let own_pid = std::process::id() as i32;

    // Gather display bounds for fullscreen detection
    let displays: objc2::rc::Retained<NSArray<SCDisplay>> = unsafe { content.displays() };
    let mut display_bounds: Vec<(f64, f64, f64, f64)> = Vec::new();
    for i in 0..displays.count() {
        let display = unsafe { displays.objectAtIndex_unchecked(i) };
        let w = unsafe { display.width() } as f64;
        let h = unsafe { display.height() } as f64;
        // SCDisplay doesn't expose origin — use CoreGraphics for multi-monitor origins
        // For now, use (0,0) for primary and just match on size
        display_bounds.push((0.0, 0.0, w, h));
    }

    let windows: objc2::rc::Retained<NSArray<SCWindow>> = unsafe { content.windows() };
    let mut results = Vec::new();

    for i in 0..windows.count() {
        let window = unsafe { windows.objectAtIndex_unchecked(i) };

        // Get owning application
        let Some(app) = (unsafe { window.owningApplication() }) else {
            continue;
        };

        // Skip our own windows
        if unsafe { app.processID() } == own_pid {
            continue;
        }

        // Get title — skip empty
        let title = unsafe { window.title() }
            .map(|t| t.to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let owner_name = unsafe { app.applicationName() }
            .map(|n| n.to_string())
            .unwrap_or_default();

        let bundle_id = unsafe { app.bundleIdentifier() }
            .map(|b| b.to_string())
            .unwrap_or_default();

        let window_id = unsafe { window.windowID() } as u32;

        // Get bounds via frame
        let frame = unsafe { window.frame() };
        let bounds = (
            frame.origin.x,
            frame.origin.y,
            frame.size.width,
            frame.size.height,
        );

        // Filter tiny windows
        if bounds.2 < MIN_WINDOW_DIMENSION || bounds.3 < MIN_WINDOW_DIMENSION {
            continue;
        }

        let is_on_screen = unsafe { window.isOnScreen() };
        let is_fullscreen = is_window_fullscreen(bounds, &display_bounds);

        results.push(WindowInfo {
            window_id,
            title,
            owner_name,
            bundle_id,
            bounds,
            is_on_screen,
            is_fullscreen,
        });
    }

    results
}
```

Note: `SCWindow::frame()`, `SCWindow::title()`, `SCWindow::isOnScreen()`, `SCWindow::windowID()`, and `SCRunningApplication::bundleIdentifier()` are available in `objc2-screen-capture-kit 0.3`. The implementer should check the exact method signatures against the crate docs and may need to enable additional features in `Cargo.toml` (e.g., `"SCWindow"`, `"SCRunningApplication"`). Add feature flags as needed.

- [ ] **Step 4: Add `enumerate_applications()` function**

```rust
/// Enumerate running applications with capturable windows, grouped by bundle ID.
#[cfg(target_os = "macos")]
pub fn enumerate_applications() -> Vec<AppInfo> {
    let windows = enumerate_windows();
    let mut apps: std::collections::HashMap<String, AppInfo> = std::collections::HashMap::new();

    for win in windows {
        let entry = apps.entry(win.bundle_id.clone()).or_insert_with(|| AppInfo {
            bundle_id: win.bundle_id.clone(),
            name: win.owner_name.clone(),
            windows: Vec::new(),
        });
        entry.windows.push(win);
    }

    let mut result: Vec<AppInfo> = apps.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}
```

- [ ] **Step 5: Add fullscreen detection helper**

```rust
/// Check if a window's bounds cover an entire display (native or borderless fullscreen).
fn is_window_fullscreen(
    window_bounds: (f64, f64, f64, f64),
    displays: &[(f64, f64, f64, f64)], // display bounds: x, y, width, height
) -> bool {
    let (wx, wy, ww, wh) = window_bounds;
    for &(dx, dy, dw, dh) in displays {
        // Allow 1px tolerance for rounding
        if (wx - dx).abs() <= 1.0
            && (wy - dy).abs() <= 1.0
            && (ww - dw).abs() <= 1.0
            && (wh - dh).abs() <= 1.0
        {
            return true;
        }
    }
    false
}
```

- [ ] **Step 6: Update tests**

Update `enumerate_windows_does_not_panic` test to check new fields:

```rust
#[test]
fn enumerate_windows_does_not_panic() {
    let windows = enumerate_windows();
    for w in &windows {
        assert!(!w.title.is_empty());
        assert!(w.window_id != 0);
        // bundle_id may be empty for some system windows
    }
}
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 8: Commit**

```bash
git add src/gstreamer/devices.rs
git commit -m "feat: enhanced window enumeration with bundle ID and fullscreen detection"
```

---

### Task 3: ScreenCaptureKit Window Capture (`screencapturekit.rs`)

**Files:**
- Modify: `src/gstreamer/screencapturekit.rs`
- Modify: `Cargo.toml:40` (may need SCContentFilter features)

- [ ] **Step 1: Add `CaptureKind` enum and update `SCStreamHandle`**

```rust
/// What kind of capture this stream handle represents.
#[derive(Debug)]
pub enum CaptureKind {
    Display { screen_index: usize },
    Window { window_id: u32 },
}

pub struct SCStreamHandle {
    stream: Retained<SCStream>,
    _delegate: Retained<StreamOutputDelegate>,
    pub kind: CaptureKind,
}
```

Remove the old `screen_index: usize` field.

- [ ] **Step 2: Update `start_display_capture` to use `CaptureKind`**

Change the handle construction at the end to use `kind: CaptureKind::Display { screen_index }`.

- [ ] **Step 3: Make `get_shareable_content()` pub(crate)**

Change `fn get_shareable_content()` to `pub(crate) fn get_shareable_content()` so devices.rs can use it.

- [ ] **Step 4: Add `start_window_capture()` function**

```rust
/// Start capturing a specific window via ScreenCaptureKit.
///
/// Uses `SCContentFilter` initialized with a single `SCWindow` for
/// desktop-independent window capture (no background, just the window content).
pub fn start_window_capture(
    window_id: u32,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(SCStreamHandle, std_mpsc::Receiver<RgbaFrame>)> {
    let content = get_shareable_content()?;

    // Find the SCWindow matching our window_id
    let windows: Retained<NSArray<SCWindow>> = unsafe { content.windows() };
    let mut target_window: Option<Retained<SCWindow>> = None;
    for i in 0..windows.count() {
        let window = unsafe { windows.objectAtIndex_unchecked(i) };
        if unsafe { window.windowID() } == window_id as u64 {
            target_window = Some(window.retain());
            break;
        }
    }

    let window = target_window.ok_or_else(|| anyhow!("Window {} not found", window_id))?;

    // Create filter for single window (desktop-independent)
    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(
            SCContentFilter::alloc(),
            &window,
        )
    };

    let config = build_stream_config(width, height, fps)?;
    let (frame_tx, frame_rx) = std_mpsc::channel();
    let delegate = StreamOutputDelegate::new(frame_tx);

    let delegate_for_stream: Retained<ProtocolObject<dyn SCStreamDelegate>> =
        ProtocolObject::from_retained(delegate.clone());
    let stream = unsafe {
        SCStream::initWithFilter_configuration_delegate(
            SCStream::alloc(),
            &filter,
            &config,
            Some(&*delegate_for_stream),
        )
    };

    let output_proto: Retained<ProtocolObject<dyn SCStreamOutput>> =
        ProtocolObject::from_retained(delegate.clone());
    unsafe {
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(
                &output_proto,
                SCStreamOutputType::Screen,
                None,
            )
            .map_err(|e| anyhow!("Failed to add stream output: {}", e))?;
    }

    // Start capture
    let (start_tx, start_rx) = std_mpsc::channel();
    let start_block = RcBlock::new(move |error: *mut NSError| {
        if error.is_null() {
            let _ = start_tx.send(Ok(()));
        } else {
            let desc = unsafe { (*error).localizedDescription().to_string() };
            let _ = start_tx.send(Err(anyhow!("Failed to start window capture: {}", desc)));
        }
    });
    unsafe {
        stream.startCaptureWithCompletionHandler(Some(&start_block));
    }
    start_rx
        .recv()
        .map_err(|_| anyhow!("Start capture channel closed"))??;

    let handle = SCStreamHandle {
        stream,
        _delegate: delegate,
        kind: CaptureKind::Window { window_id },
    };

    Ok((handle, frame_rx))
}
```

Note: Check that `initWithDesktopIndependentWindow` is available in `objc2-screen-capture-kit 0.3`. If not, fall back to `initWithWindow_desktopIndependentWindow:` or equivalent. The implementer should check the crate docs and adjust the initializer name accordingly.

- [ ] **Step 5: Add `update_window_target()` function**

```rust
/// Switch a running window capture stream to target a different window.
///
/// Builds a new `SCContentFilter` for the new window and calls
/// `updateContentFilter` on the existing stream — no teardown needed.
pub fn update_window_target(handle: &mut SCStreamHandle, new_window_id: u32) -> Result<()> {
    let content = get_shareable_content()?;
    let windows: Retained<NSArray<SCWindow>> = unsafe { content.windows() };

    let mut target_window: Option<Retained<SCWindow>> = None;
    for i in 0..windows.count() {
        let window = unsafe { windows.objectAtIndex_unchecked(i) };
        if unsafe { window.windowID() } == new_window_id as u64 {
            target_window = Some(window.retain());
            break;
        }
    }

    let window = target_window.ok_or_else(|| anyhow!("Window {} not found", new_window_id))?;

    let filter = unsafe {
        SCContentFilter::initWithDesktopIndependentWindow(
            SCContentFilter::alloc(),
            &window,
        )
    };

    let (tx, rx) = std_mpsc::channel();
    let block = RcBlock::new(move |error: *mut NSError| {
        if error.is_null() {
            let _ = tx.send(Ok(()));
        } else {
            let desc = unsafe { (*error).localizedDescription().to_string() };
            let _ = tx.send(Err(anyhow!("Failed to update window target: {}", desc)));
        }
    });
    unsafe {
        handle
            .stream
            .updateContentFilter_completionHandler(&filter, Some(&block));
    }
    rx.recv()
        .map_err(|_| anyhow!("Update filter channel closed"))??;

    handle.kind = CaptureKind::Window { window_id: new_window_id };
    Ok(())
}
```

- [ ] **Step 6: Update `update_exclusion` to use `CaptureKind`**

Replace `handle.screen_index` references with match on `handle.kind`:

```rust
pub fn update_exclusion(handle: &SCStreamHandle, exclude_own_pid: bool) -> Result<()> {
    let CaptureKind::Display { screen_index } = handle.kind else {
        return Err(anyhow!("update_exclusion only applies to display captures"));
    };
    // ... rest unchanged, using screen_index ...
}
```

- [ ] **Step 7: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 8: Commit**

```bash
git add src/gstreamer/screencapturekit.rs Cargo.toml
git commit -m "feat: add ScreenCaptureKit window capture with live target switching"
```

---

### Task 4: Window Watcher (`window_watcher.rs`)

**Files:**
- Create: `src/gstreamer/window_watcher.rs`

- [ ] **Step 1: Create the `WindowWatcher` struct**

```rust
//! Periodic window watcher that resolves the best capture target for each
//! active window source. Runs on the GStreamer thread.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use log;

use super::devices::{enumerate_applications, AppInfo, WindowInfo};
use super::screencapturekit::{self, SCStreamHandle};
use crate::scene::{SourceId, WindowCaptureMode};

/// How often to poll for window changes.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// Tracks the current state of a window capture source.
pub struct WatchedSource {
    pub mode: WindowCaptureMode,
    pub current_window_id: Option<u32>,
    pub current_window_size: Option<(u32, u32)>,
}

/// Resolves the best capture target for window sources.
pub struct WindowWatcher {
    last_poll: Instant,
    /// Cached app list from last poll.
    cached_apps: Vec<AppInfo>,
    /// Cached display bounds for fullscreen detection.
    cached_display_bounds: Vec<(f64, f64, f64, f64)>,
}

impl WindowWatcher {
    pub fn new() -> Self {
        Self {
            last_poll: Instant::now() - POLL_INTERVAL, // trigger immediate first poll
            cached_apps: Vec::new(),
            cached_display_bounds: Vec::new(),
        }
    }

    /// Called each iteration of the GStreamer thread poll loop.
    /// Returns a list of (source_id, new_window_id) pairs for sources whose
    /// target has changed.
    pub fn poll(
        &mut self,
        watched: &HashMap<SourceId, WatchedSource>,
    ) -> Vec<(SourceId, Option<u32>)> {
        if self.last_poll.elapsed() < POLL_INTERVAL || watched.is_empty() {
            return Vec::new();
        }
        self.last_poll = Instant::now();

        // Re-enumerate
        self.cached_apps = enumerate_applications();
        self.refresh_display_bounds();

        let mut changes = Vec::new();

        for (source_id, source) in watched {
            let resolved = self.resolve_target(&source.mode);
            if resolved != source.current_window_id {
                changes.push((*source_id, resolved));
            }
        }

        changes
    }

    /// Force an immediate refresh of cached app/display data.
    pub fn force_refresh(&mut self) {
        self.cached_apps = enumerate_applications();
        self.refresh_display_bounds();
        self.last_poll = Instant::now();
    }

    /// Resolve the best window ID for the given capture mode.
    pub fn resolve_target(&self, mode: &WindowCaptureMode) -> Option<u32> {
        match mode {
            WindowCaptureMode::AnyFullscreen => self.find_fullscreen_window(),
            WindowCaptureMode::Application {
                bundle_id,
                pinned_title,
                ..
            } => self.find_app_window(bundle_id, pinned_title.as_deref()),
        }
    }

    /// Find the frontmost fullscreen window (native or borderless).
    fn find_fullscreen_window(&self) -> Option<u32> {
        // Collect all fullscreen windows across all apps
        let mut fullscreen: Vec<&WindowInfo> = self
            .cached_apps
            .iter()
            .flat_map(|app| &app.windows)
            .filter(|w| w.is_fullscreen && w.is_on_screen)
            .collect();

        // Return first (frontmost — enumeration order from SCK is front-to-back)
        fullscreen.first().map(|w| w.window_id)
    }

    /// Find the best window for a specific application.
    fn find_app_window(&self, bundle_id: &str, pinned_title: Option<&str>) -> Option<u32> {
        let app = self.cached_apps.iter().find(|a| a.bundle_id == bundle_id)?;

        if app.windows.is_empty() {
            return None;
        }

        // If pinned, try to find matching window first
        if let Some(title) = pinned_title {
            if let Some(win) = app.windows.iter().find(|w| w.title.contains(title)) {
                return Some(win.window_id);
            }
            // Pinned window not found — fall through to frontmost
        }

        // Return frontmost (first on-screen window)
        app.windows
            .iter()
            .find(|w| w.is_on_screen)
            .or(app.windows.first())
            .map(|w| w.window_id)
    }

    /// Refresh cached display bounds for fullscreen detection.
    fn refresh_display_bounds(&mut self) {
        // Use SCK display enumeration to get display bounds.
        // This is called every poll interval so it picks up monitor changes.
        match screencapturekit::enumerate_displays() {
            Ok(displays) => {
                self.cached_display_bounds = displays
                    .iter()
                    .map(|d| (0.0, 0.0, d.width as f64, d.height as f64))
                    .collect();
            }
            Err(e) => {
                log::warn!("Failed to enumerate displays for fullscreen detection: {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/gstreamer/mod.rs`, add:

```rust
pub mod window_watcher;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/window_watcher.rs src/gstreamer/mod.rs
git commit -m "feat: add WindowWatcher for dynamic application tracking"
```

---

### Task 5: Update GStreamer Commands (`commands.rs`)

**Files:**
- Modify: `src/gstreamer/commands.rs:97-116`

- [ ] **Step 1: Update `CaptureSourceConfig::Window`**

```rust
Window {
    mode: crate::scene::WindowCaptureMode,
},
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 3: Commit**

```bash
git add src/gstreamer/commands.rs
git commit -m "refactor: update CaptureSourceConfig::Window to use WindowCaptureMode"
```

---

### Task 6: Update GStreamer Thread (`thread.rs`)

**Files:**
- Modify: `src/gstreamer/thread.rs`

- [ ] **Step 1: Add WindowWatcher and watched sources to `GstThread`**

Add fields to `GstThread`:

```rust
use super::window_watcher::{WindowWatcher, WatchedSource};

struct GstThread {
    // ... existing fields ...
    /// Watcher that resolves window capture targets.
    window_watcher: WindowWatcher,
    /// Active window capture sources being watched.
    watched_windows: HashMap<SourceId, WatchedSource>,
}
```

Initialize in `GstThread::new()`:

```rust
window_watcher: WindowWatcher::new(),
watched_windows: HashMap::new(),
```

- [ ] **Step 2: Rewrite `add_window_capture_source()` to use ScreenCaptureKit**

Replace the entire `add_window_capture_source` method. The new version:
1. Receives a `WindowCaptureMode` instead of a raw `window_id`
2. Registers the source with the `WindowWatcher`
3. Forces an immediate watcher refresh and resolves the initial target using `self.window_watcher.resolve_target()`
4. If a window is found, starts SCK capture (same pattern as `add_display_capture_source`)
5. If no window found (app not running), registers the watcher entry anyway — it will start capture when the app appears

```rust
#[cfg(target_os = "macos")]
fn add_window_capture_source(
    &mut self,
    source_id: SourceId,
    mode: crate::scene::WindowCaptureMode,
) {
    use super::window_watcher::WatchedSource;

    // Register with watcher
    let watched = WatchedSource {
        mode: mode.clone(),
        current_window_id: None,
        current_window_size: None,
    };
    self.watched_windows.insert(source_id, watched);

    // Force watcher to refresh its app cache and resolve
    self.window_watcher.force_refresh();
    let initial_window_id = self.window_watcher.resolve_target(&mode);

    let Some(window_id) = initial_window_id else {
        log::info!("No window found for source {source_id:?}, watcher will retry");
        return;
    };

    self.start_sck_window_capture(source_id, window_id);
}
```

- [ ] **Step 3: Add `start_sck_window_capture` helper**

```rust
#[cfg(target_os = "macos")]
fn start_sck_window_capture(&mut self, source_id: SourceId, window_id: u32) {
    use super::capture::build_display_capture_pipeline;
    use super::screencapturekit;

    let width = 1920u32;
    let height = 1080u32;
    let fps = 30u32;

    let (sck_handle, frame_rx) = match screencapturekit::start_window_capture(
        window_id, width, height, fps,
    ) {
        Ok(result) => result,
        Err(e) => {
            log::error!("Window capture failed for source {source_id:?}: {e}");
            let _ = self.channels.error_tx.send(super::error::GstError::CaptureFailure {
                message: format!("Window capture failed: {e}"),
            });
            return;
        }
    };

    let (pipeline, appsink, appsrc) = match build_display_capture_pipeline(width, height, fps) {
        Ok(result) => result,
        Err(e) => {
            log::error!("Failed to build window pipeline for source {source_id:?}: {e}");
            let _ = screencapturekit::stop_display_capture(sck_handle);
            return;
        }
    };

    if let Err(e) = pipeline.set_state(gstreamer::State::Playing) {
        log::error!("Failed to start window capture for source {source_id:?}: {e}");
        let _ = screencapturekit::stop_display_capture(sck_handle);
        return;
    }

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);

    std::thread::Builder::new()
        .name(format!("window-capture-{window_id}"))
        .spawn(move || {
            log::info!("Window capture pump started for window {window_id}");
            while running_clone.load(Ordering::Relaxed) {
                match frame_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(frame) => {
                        let mut buffer = gstreamer::Buffer::with_size(frame.data.len()).unwrap();
                        {
                            let buf_ref = buffer.get_mut().unwrap();
                            let mut map = buf_ref.map_writable().unwrap();
                            map.as_mut_slice().copy_from_slice(&frame.data);
                        }
                        if appsrc.push_buffer(buffer).is_err() {
                            log::warn!("Failed to push buffer to window appsrc, stopping");
                            break;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        log::warn!("Window capture channel disconnected");
                        break;
                    }
                }
            }
            log::info!("Window capture pump exiting for window {window_id}");
        })
        .expect("spawn window capture pump thread");

    // Update watched state
    if let Some(watched) = self.watched_windows.get_mut(&source_id) {
        watched.current_window_id = Some(window_id);
    }

    self.captures.insert(
        source_id,
        CaptureHandle {
            pipeline,
            appsink,
            capture_running: Some(running),
            sck_handle: Some(sck_handle),
        },
    );
    log::info!("Window capture started for source {source_id:?} (window {window_id})");
}
```

- [ ] **Step 4: Integrate watcher into the poll loop**

Find the main poll loop in `GstThread` (the `run()` or equivalent method). Add a watcher poll call. After processing commands, call:

```rust
// Poll window watcher for target changes
let changes = self.window_watcher.poll(&self.watched_windows);
for (source_id, new_window_id) in changes {
    self.handle_window_target_change(source_id, new_window_id);
}
```

- [ ] **Step 5: Add `handle_window_target_change` method**

```rust
fn handle_window_target_change(&mut self, source_id: SourceId, new_window_id: Option<u32>) {
    match new_window_id {
        Some(wid) => {
            // Check if we have an existing capture we can retarget
            if let Some(capture) = self.captures.get_mut(&source_id) {
                if let Some(ref mut sck_handle) = capture.sck_handle {
                    match super::screencapturekit::update_window_target(sck_handle, wid) {
                        Ok(()) => {
                            if let Some(watched) = self.watched_windows.get_mut(&source_id) {
                                watched.current_window_id = Some(wid);
                            }
                            log::info!("Switched window target for {source_id:?} to {wid}");
                            return;
                        }
                        Err(e) => {
                            log::warn!("Failed to update window target, rebuilding: {e}");
                        }
                    }
                }
            }
            // No existing capture or retarget failed — start fresh
            self.remove_capture_source(source_id);
            self.start_sck_window_capture(source_id, wid);
        }
        None => {
            // App closed — hold last frame, don't tear down
            log::info!("Target window gone for {source_id:?}, holding last frame");
        }
    }
}
```

- [ ] **Step 6: Clean up watched_windows on source removal**

In `remove_capture_source()`, add:

```rust
self.watched_windows.remove(&source_id);
```

- [ ] **Step 7: Update the `CaptureSourceConfig::Window` match in `add_capture_source()`**

```rust
#[cfg(target_os = "macos")]
if let CaptureSourceConfig::Window { mode } = config {
    self.add_window_capture_source(source_id, mode.clone());
    return;
}
```

- [ ] **Step 8: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 9: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "feat: integrate WindowWatcher for dynamic window capture on GStreamer thread"
```

---

### Task 7: Remove Old CoreGraphics Window Capture (`capture.rs`)

**Files:**
- Modify: `src/gstreamer/capture.rs`

- [ ] **Step 1: Remove `grab_window_frame()` function**

Delete the `grab_window_frame()` function (lines 101-144).

- [ ] **Step 2: Remove `build_window_capture_pipeline()` function**

Delete the `build_window_capture_pipeline()` function (lines 221-275).

- [ ] **Step 3: Remove the `CaptureSourceConfig::Window` bail in `build_capture_pipeline()`**

The match arm at lines 30-32 can remain as-is (it already bails), or remove it since window capture is now fully handled before `build_capture_pipeline` is called. Keep the bail for safety.

- [ ] **Step 4: Remove unused imports**

Check for `core_graphics` imports that are no longer needed.

- [ ] **Step 5: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 6: Commit**

```bash
git add src/gstreamer/capture.rs
git commit -m "refactor: remove CoreGraphics window capture in favor of ScreenCaptureKit"
```

---

### Task 8: Update UI — Properties Panel (`properties_panel.rs`)

**Files:**
- Modify: `src/ui/properties_panel.rs:560-638`

- [ ] **Step 1: Replace the window properties section**

Replace the entire `SourceType::Window` block. The new UI should have:

1. A mode selector: "Specific Application" / "Any Fullscreen Application"
2. When "Specific Application": an app dropdown (grouped by bundle_id), populated from `enumerate_applications()`
3. When an app is selected and has multiple windows: a "Pin to window" toggle with window title dropdown
4. Status text showing current state ("Capturing: Chrome", "Waiting for app...", "No fullscreen app")

```rust
SourceType::Window => {
    section_label(ui, "SOURCE");
    ui.add_space(4.0);

    // Cache apps list (same pattern as cameras/windows)
    if state.available_apps.is_empty() {
        state.available_apps = crate::gstreamer::devices::enumerate_applications();
    }
    let apps = state.available_apps.clone();
    let cmd_tx = state.command_tx.clone();

    let source = &mut state.library[lib_idx];
    let SourceProperties::Window {
        ref mut mode,
        ref current_window_id,
    } = source.properties
    else {
        return changed;
    };

    let prev_mode = mode.clone();

    // Mode selector
    let is_fullscreen_mode = matches!(mode, WindowCaptureMode::AnyFullscreen);
    let mode_label = if is_fullscreen_mode {
        "Any Fullscreen Application"
    } else {
        "Specific Application"
    };

    egui::ComboBox::from_id_salt(
        egui::Id::new("props_window_mode").with(selected_id.0),
    )
    .selected_text(mode_label)
    .width(ui.available_width())
    .show_ui(ui, |ui| {
        if ui.selectable_label(!is_fullscreen_mode, "Specific Application").clicked() {
            if is_fullscreen_mode {
                *mode = WindowCaptureMode::Application {
                    bundle_id: String::new(),
                    app_name: String::new(),
                    pinned_title: None,
                };
            }
        }
        if ui.selectable_label(is_fullscreen_mode, "Any Fullscreen Application").clicked() {
            if !is_fullscreen_mode {
                *mode = WindowCaptureMode::AnyFullscreen;
            }
        }
    });

    ui.add_space(4.0);

    // App selector (only in Application mode)
    if let WindowCaptureMode::Application {
        ref mut bundle_id,
        ref mut app_name,
        ref mut pinned_title,
    } = mode
    {
        let selected_app_label = if app_name.is_empty() {
            "Select an application...".to_string()
        } else {
            app_name.clone()
        };

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(
                egui::Id::new("props_window_app").with(selected_id.0),
            )
            .selected_text(&selected_app_label)
            .width(ui.available_width() - 32.0)
            .show_ui(ui, |ui| {
                for app in &apps {
                    if ui
                        .selectable_label(*bundle_id == app.bundle_id, &app.name)
                        .clicked()
                    {
                        *bundle_id = app.bundle_id.clone();
                        *app_name = app.name.clone();
                        *pinned_title = None;
                    }
                }
            });

            // Refresh button
            if ui
                .button(
                    egui::RichText::new(egui_phosphor::regular::ARROW_CLOCKWISE)
                        .size(14.0)
                        .color(theme.text_secondary),
                )
                .on_hover_text("Refresh application list")
                .clicked()
            {
                state.available_apps = crate::gstreamer::devices::enumerate_applications();
            }
        });

        // Pin-to-window toggle (when app has multiple windows)
        if !bundle_id.is_empty() {
            if let Some(app) = apps.iter().find(|a| a.bundle_id == *bundle_id) {
                if app.windows.len() > 1 {
                    ui.add_space(4.0);
                    let mut is_pinned = pinned_title.is_some();
                    if ui.checkbox(&mut is_pinned, "Pin to specific window").changed() {
                        if is_pinned {
                            *pinned_title = app.windows.first().map(|w| w.title.clone());
                        } else {
                            *pinned_title = None;
                        }
                    }

                    if let Some(ref mut title) = pinned_title {
                        egui::ComboBox::from_id_salt(
                            egui::Id::new("props_window_pin").with(selected_id.0),
                        )
                        .selected_text(title.as_str())
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for win in &app.windows {
                                if ui.selectable_label(*title == win.title, &win.title).clicked() {
                                    *title = win.title.clone();
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    // Status display
    ui.add_space(4.0);
    let status = if current_window_id.is_some() {
        match mode {
            WindowCaptureMode::Application { app_name, .. } => {
                format!("Capturing: {app_name}")
            }
            WindowCaptureMode::AnyFullscreen => "Capturing fullscreen app".to_string(),
        }
    } else {
        match mode {
            WindowCaptureMode::Application { app_name, .. } if !app_name.is_empty() => {
                format!("Waiting for {}...", app_name)
            }
            WindowCaptureMode::AnyFullscreen => "No fullscreen application".to_string(),
            _ => "Select an application".to_string(),
        }
    };
    ui.label(
        egui::RichText::new(&status)
            .size(11.0)
            .color(if current_window_id.is_some() {
                theme.text_secondary
            } else {
                theme.text_muted
            }),
    );

    // Trigger capture restart if mode changed
    if *mode != prev_mode {
        if let Some(ref tx) = cmd_tx {
            let _ = tx.try_send(GstCommand::RemoveCaptureSource {
                source_id: selected_id,
            });
            let _ = tx.try_send(GstCommand::AddCaptureSource {
                source_id: selected_id,
                config: CaptureSourceConfig::Window {
                    mode: mode.clone(),
                },
            });
        }
        changed = true;
    }
}
```

- [ ] **Step 2: Add required imports at the top of properties_panel.rs**

Add `use crate::scene::WindowCaptureMode;` if not already imported.

- [ ] **Step 3: Verify compilation**

Run: `cargo check 2>&1 | head -60`

- [ ] **Step 4: Commit**

```bash
git add src/ui/properties_panel.rs
git commit -m "feat: replace window dropdown with application selector and pin-to-window"
```

---

### Task 9: Update Remaining UI Files, State, and Main (`sources_panel.rs`, `state.rs`, `main.rs`, `gstreamer/mod.rs`)

**Files:**
- Modify: `src/ui/sources_panel.rs:682-689`
- Modify: `src/state.rs:134,197`
- Modify: `src/main.rs:197,244,571`
- Modify: `src/gstreamer/mod.rs:20`

Note: `scenes_panel.rs:446,564`, `library_panel.rs:296`, `preview_panel.rs:841`, and `main.rs:571` were already fixed in Task 1 Step 4.

- [ ] **Step 1: Update `start_capture_from_properties()` in sources_panel.rs**

Replace the Window match arm (line 682):

```rust
SourceProperties::Window { ref mode, .. } => {
    let _ = tx.try_send(GstCommand::AddCaptureSource {
        source_id,
        config: CaptureSourceConfig::Window {
            mode: mode.clone(),
        },
    });
    state.capture_active = true;
}
```

- [ ] **Step 2: Update `AppState` in state.rs**

Replace `available_windows` field (line 134):

```rust
pub available_apps: Vec<crate::gstreamer::devices::AppInfo>,
```

Update the Default impl (line 197) to use `Vec::new()`.

- [ ] **Step 3: Update `main.rs` startup**

Replace the window enumeration (line 197):

```rust
let available_apps = crate::gstreamer::devices::enumerate_applications();
log::info!("Found {} application(s)", available_apps.len());
```

Update the `AppState` construction (line 244) to use `available_apps`.

- [ ] **Step 4: Update `gstreamer/mod.rs` re-exports**

Add `AppInfo` to the re-export at line 20:

```rust
pub use devices::{AppInfo, CameraDevice, DisplayInfo, WindowInfo};
```

- [ ] **Step 5: Fix any remaining compilation errors**

Run: `cargo check 2>&1 | head -60`

Fix any remaining references to `available_windows` or old patterns.

- [ ] **Step 6: Commit**

```bash
git add src/ui/sources_panel.rs src/state.rs src/main.rs src/gstreamer/mod.rs
git commit -m "refactor: update source panel, state, and main for dynamic window capture"
```

---

### Task 10: Integration Test and Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Full build check**

Run: `cargo build 2>&1 | tail -20`
Expected: Clean build with no errors.

- [ ] **Step 2: Run existing tests**

Run: `cargo test 2>&1 | tail -30`
Expected: All existing tests pass.

- [ ] **Step 3: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Fix any warnings.

- [ ] **Step 4: Check formatting**

Run: `cargo fmt --check`
Fix if needed.

- [ ] **Step 5: Manual smoke test**

Run: `cargo run`
1. Add a new Window source from the library
2. Verify the mode selector shows "Specific Application" / "Any Fullscreen Application"
3. Select an app from the dropdown — verify capture starts
4. Close and reopen the captured app — verify capture resumes
5. Test "Any Fullscreen Application" with a fullscreen app
6. Test pin-to-window with a multi-window app

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "feat: dynamic window source with application tracking and fullscreen detection"
```
