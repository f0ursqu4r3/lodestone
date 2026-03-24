# Virtual Camera Output — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Lodestone's composited output available as a virtual camera that any app (Zoom, Discord, FaceTime) can select as a camera source.

**Architecture:** Two binaries — the main Rust app writes composited frames to an IOSurface (shared memory), and a Swift Camera Extension reads them and presents as "Lodestone Virtual Camera" to the system. Frame delivery is polling-based (extension reads at its own cadence). IOSurface ID shared via App Group UserDefaults.

**Tech Stack:** Rust (IOSurface FFI via `core-graphics`/`objc2`), Swift (CMIOExtensionProvider), IOSurface (shared memory IPC), App Group (cross-process sharing)

---

## File Structure

### New files:
- `src/gstreamer/virtual_camera.rs` — IOSurface lifecycle, frame writing, macOS-only
- `Lodestone.entitlements` — main app entitlements (system extension install + app group)
- `lodestone-camera-extension/` — Swift Camera Extension Xcode project
  - `lodestone-camera-extension/LodestoneCamera.xcodeproj` — Xcode project
  - `lodestone-camera-extension/Sources/main.swift` — Extension entry point
  - `lodestone-camera-extension/Sources/Provider.swift` — CMIOExtensionProviderSource
  - `lodestone-camera-extension/Sources/Device.swift` — CMIOExtensionDeviceSource
  - `lodestone-camera-extension/Sources/Stream.swift` — CMIOExtensionStreamSource, frame polling
  - `lodestone-camera-extension/Info.plist` — Extension metadata
  - `lodestone-camera-extension/LodestoneCamera.entitlements` — Required entitlements

### Modified files:
- `src/gstreamer/commands.rs` — add StartVirtualCamera/StopVirtualCamera commands
- `src/gstreamer/thread.rs` — handle commands, route composited frames, store handle
- `src/gstreamer/mod.rs` — register virtual_camera module
- `src/state.rs` — add virtual_camera_active field
- `src/ui/toolbar.rs` — add Virtual Camera toggle button
- `src/main.rs` — add virtual_camera_active to is_encoding condition

---

## Task 1: Add GstCommands and state for virtual camera

**Files:**
- Modify: `src/gstreamer/commands.rs`
- Modify: `src/state.rs`

- [ ] **Step 1: Add command variants**

In `src/gstreamer/commands.rs`, add before the `Shutdown` variant (~line 76):

```rust
StartVirtualCamera,
StopVirtualCamera,
```

- [ ] **Step 2: Add state field**

In `src/state.rs`, add to the `AppState` struct (after `recording_status`):

```rust
pub virtual_camera_active: bool,
```

In the `Default` impl, add:

```rust
virtual_camera_active: false,
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`
Expected: Clean build, 125 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/gstreamer/commands.rs src/state.rs
git commit -m "feat: add virtual camera command variants and state"
```

---

## Task 2: Virtual camera IOSurface module

**Files:**
- Create: `src/gstreamer/virtual_camera.rs`
- Modify: `src/gstreamer/mod.rs`

This is the core Rust module that manages IOSurface lifecycle and frame writing.

- [ ] **Step 1: Add dependencies to Cargo.toml**

Add `objc2-io-surface` for IOSurface FFI and add `"NSUserDefaults"` to the existing `objc2-foundation` features:

```toml
objc2-io-surface = { version = "0.3", features = ["IOSurface"] }
```

Update the existing `objc2-foundation` line to include `"NSUserDefaults"`:
```toml
objc2-foundation = { version = "0.3", features = ["NSArray", "NSError", "NSString", "NSThread", "NSUserDefaults"] }
```

- [ ] **Step 2: Create `src/gstreamer/virtual_camera.rs`**

The module provides three functions. Use `objc2_io_surface` for IOSurface bindings (not `core_graphics::sys` which has no IOSurface support). The IOSurface handle should use `Retained<IOSurface>` (the `objc2` owned type), not a raw pointer.

```rust
//! Virtual camera output via IOSurface shared memory (macOS only).
//!
//! Creates an IOSurface, publishes its ID to App Group UserDefaults,
//! and writes composited RGBA frames (converted to BGRA) for the
//! Camera Extension to read.

use anyhow::{Result, anyhow};
use objc2::rc::Retained;
use objc2_foundation::NSUserDefaults;
use objc2_io_surface::IOSurface;
use std::sync::atomic::{AtomicU64, Ordering};

use super::types::RgbaFrame;

/// App Group suite name shared between main app and Camera Extension.
const APP_GROUP_SUITE: &str = "group.com.lodestone.app";
/// UserDefaults key for the IOSurface ID.
const SURFACE_ID_KEY: &str = "virtualCameraSurfaceID";
/// UserDefaults key for the active frame width.
const FRAME_WIDTH_KEY: &str = "virtualCameraWidth";
/// UserDefaults key for the active frame height.
const FRAME_HEIGHT_KEY: &str = "virtualCameraHeight";

/// Handle to a running virtual camera output.
pub struct VirtualCameraHandle {
    surface: Retained<IOSurface>,
    width: u32,
    height: u32,
    fps: u32,
    frame_counter: AtomicU64,
}

// SAFETY: IOSurface is thread-safe (backed by kernel-managed shared memory).
unsafe impl Send for VirtualCameraHandle {}

/// Start the virtual camera: create an IOSurface and publish its ID.
pub fn start_virtual_camera(width: u32, height: u32, fps: u32) -> Result<VirtualCameraHandle> {
    // 1. Create IOSurface with properties:
    //    - kIOSurfaceWidth: width
    //    - kIOSurfaceHeight: height
    //    - kIOSurfaceBytesPerElement: 4 (BGRA)
    //    - kIOSurfaceBytesPerRow: width * 4
    //    - kIOSurfaceAllocSize: width * height * 4
    //    - kIOSurfacePixelFormat: 'BGRA' (0x42475241)
    //
    // 2. Get IOSurfaceID via IOSurfaceGetID()
    //
    // 3. Write ID + dimensions to App Group UserDefaults:
    //    NSUserDefaults(suiteName: APP_GROUP_SUITE)
    //      .set(surface_id, forKey: SURFACE_ID_KEY)
    //      .set(width, forKey: FRAME_WIDTH_KEY)
    //      .set(height, forKey: FRAME_HEIGHT_KEY)
    //
    // 4. Return handle

    todo!("IOSurface creation + App Group UserDefaults publishing")
}

/// Write a composited frame to the IOSurface.
///
/// Converts RGBA to BGRA, locks the surface for writing, copies data,
/// unlocks, and increments the frame counter.
pub fn write_frame(handle: &VirtualCameraHandle, frame: &RgbaFrame) -> Result<()> {
    // 1. Validate frame dimensions match surface
    // 2. IOSurfaceLock(surface, 0) — write lock
    // 3. Get base address via IOSurfaceGetBaseAddress()
    // 4. Copy frame.data with RGBA→BGRA byte swap (swap [0] and [2] per pixel)
    //    For frames smaller than surface, write to top-left region
    // 5. IOSurfaceUnlock(surface, 0)
    // 6. Increment frame counter (atomic)
    // 7. Update IOSurface seed value so extension can detect new frames

    todo!("Frame write with BGRA conversion")
}

/// Stop the virtual camera: release the IOSurface and clear published ID.
pub fn stop_virtual_camera(handle: VirtualCameraHandle) -> Result<()> {
    // 1. Clear UserDefaults entries (set to 0 / remove)
    // 2. Release IOSurface (CFRelease)

    todo!("Cleanup")
}
```

The `todo!()` stubs will be filled in during implementation. The implementer should use `core_graphics::sys` for raw IOSurface FFI calls, or the `objc2` crate's `msg_send!` macro for `NSUserDefaults` access. The existing `screencapturekit.rs` module demonstrates the ObjC interop pattern.

**Key `objc2_io_surface::IOSurface` methods**:
- `IOSurface::new(properties)` — create surface
- `surface.id()` or `IOSurfaceGetID()` — get uint32 global token
- `surface.lock(options)` / `surface.unlock(options)` — read/write locking
- `surface.base_address()` — get pixel data pointer
- `surface.bytes_per_row()` — stride

**NSUserDefaults** (via `objc2_foundation`):
- `NSUserDefaults::initWithSuiteName(alloc, &NSString::from_str(APP_GROUP_SUITE))`
- `defaults.setInteger_forKey(id as isize, &NSString::from_str(SURFACE_ID_KEY))`

- [ ] **Step 3: Register module in `src/gstreamer/mod.rs`**

Add:
```rust
#[cfg(target_os = "macos")]
pub mod virtual_camera;
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: Compiles (module has no callers yet, `todo!()` won't panic at compile time).

- [ ] **Step 5: Implement IOSurface creation in `start_virtual_camera`**

Replace the `todo!()` with actual IOSurface creation, ID publishing to UserDefaults. Test by logging the surface ID.

- [ ] **Step 6: Implement `write_frame`**

Replace the `todo!()` with lock/memcpy/unlock logic. The RGBA→BGRA swap is identical to the pattern in `capture.rs` (swap bytes [0] and [2] for each 4-byte pixel).

- [ ] **Step 7: Implement `stop_virtual_camera`**

Replace the `todo!()` with UserDefaults cleanup and IOSurface release.

- [ ] **Step 8: Build and test**

Run: `cargo build && cargo test`
Expected: Clean build.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/gstreamer/virtual_camera.rs src/gstreamer/mod.rs
git commit -m "feat: add virtual camera IOSurface module"
```

---

## Task 3: GStreamer thread integration

**Files:**
- Modify: `src/gstreamer/thread.rs`

Wire up the virtual camera to the composited frame loop and command handling.

- [ ] **Step 1: Add VirtualCameraHandle field to GstThread**

After `record_handles` (~line 46), add:

```rust
#[cfg(target_os = "macos")]
virtual_camera_handle: Option<super::virtual_camera::VirtualCameraHandle>,
```

Initialize to `None` in the constructor.

- [ ] **Step 2: Add command handlers**

Add handler methods:

```rust
#[cfg(target_os = "macos")]
fn handle_start_virtual_camera(&mut self) {
    use super::virtual_camera;
    let width = self.encoder_config.width;
    let height = self.encoder_config.height;
    let fps = self.encoder_config.fps;
    match virtual_camera::start_virtual_camera(width, height, fps) {
        Ok(handle) => {
            self.virtual_camera_handle = Some(handle);
            log::info!("Virtual camera started ({width}x{height})");
        }
        Err(e) => {
            log::error!("Failed to start virtual camera: {e}");
        }
    }
}

#[cfg(target_os = "macos")]
fn handle_stop_virtual_camera(&mut self) {
    use super::virtual_camera;
    if let Some(handle) = self.virtual_camera_handle.take() {
        if let Err(e) = virtual_camera::stop_virtual_camera(handle) {
            log::warn!("Error stopping virtual camera: {e}");
        }
        log::info!("Virtual camera stopped");
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_start_virtual_camera(&mut self) {}
#[cfg(not(target_os = "macos"))]
fn handle_stop_virtual_camera(&mut self) {}
```

- [ ] **Step 3: Route commands in `handle_command()`**

Add before the `Shutdown` arm:

```rust
GstCommand::StartVirtualCamera => self.handle_start_virtual_camera(),
GstCommand::StopVirtualCamera => self.handle_stop_virtual_camera(),
```

- [ ] **Step 4: Add frame routing in composited frame loop**

In the frame consumption loop (~line 798), after the record pipeline push, add:

```rust
#[cfg(target_os = "macos")]
if let Some(ref handle) = self.virtual_camera_handle {
    if let Err(e) = super::virtual_camera::write_frame(handle, &frame) {
        log::warn!("Virtual camera frame write failed: {e}");
    }
}
```

- [ ] **Step 5: Stop virtual camera in shutdown handler**

In `handle_shutdown()`, add before the capture drain:

```rust
self.handle_stop_virtual_camera();
```

- [ ] **Step 6: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 7: Commit**

```bash
git add src/gstreamer/thread.rs
git commit -m "feat: integrate virtual camera into GStreamer thread"
```

---

## Task 4: App integration — toolbar button and encoding trigger

**Files:**
- Modify: `src/ui/toolbar.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add Virtual Camera button to toolbar**

In `src/ui/toolbar.rs`, add a `draw_virtual_camera_button` function following the pattern of `draw_record_button` (~line 228). Use the Phosphor camera/webcam icon:

```rust
fn draw_virtual_camera_button(ui: &mut egui::Ui, state: &mut AppState) {
    let is_active = state.virtual_camera_active;
    let icon = if is_active {
        egui_phosphor::regular::WEBCAM    // or VIDEO_CAMERA
    } else {
        egui_phosphor::regular::WEBCAM
    };

    // Build button with appropriate styling (green when active, muted when off)
    // On click:
    //   if active: send StopVirtualCamera, set state.virtual_camera_active = false
    //   if inactive: send StartVirtualCamera, set state.virtual_camera_active = true
}
```

Call this function in the toolbar's `draw()` function, alongside the existing stream/record buttons.

- [ ] **Step 2: Update is_encoding condition in main.rs**

Find the `is_encoding` block (~line 848):

```rust
let is_encoding = app_state.stream_status.is_live()
    || matches!(
        app_state.recording_status,
        crate::state::RecordingStatus::Recording { .. }
    );
```

Add virtual camera:

```rust
let is_encoding = app_state.stream_status.is_live()
    || matches!(
        app_state.recording_status,
        crate::state::RecordingStatus::Recording { .. }
    )
    || app_state.virtual_camera_active;
```

- [ ] **Step 3: Build and test**

Run: `cargo build && cargo test`

- [ ] **Step 4: Commit**

```bash
git add src/ui/toolbar.rs src/main.rs
git commit -m "feat: add virtual camera toolbar button and encoding trigger"
```

---

## Task 5: Camera Extension — Swift project

**Files:**
- Create: `lodestone-camera-extension/` directory and all Swift files

This is the separate Swift binary that registers as a system camera and reads frames from the shared IOSurface.

- [ ] **Step 1: Create project structure**

```
lodestone-camera-extension/
├── Package.swift
├── Info.plist
├── LodestoneCamera.entitlements
└── Sources/
    ├── main.swift
    ├── Provider.swift
    ├── Device.swift
    └── Stream.swift
```

- [ ] **Step 2: Create Xcode project**

CMIOExtension targets require an Xcode project — Swift Package Manager cannot produce `.systemextension` bundles with the required `NSExtension` principal class, entitlements, and code signing. Create an Xcode project (`LodestoneCamera.xcodeproj`) with a "Camera Extension" target. Xcode has a template for this under File → New Target → Camera Extension.

- [ ] **Step 3: Create main.swift**

Entry point that starts the `CMIOExtensionProvider`:

```swift
import CoreMediaIO
import Foundation

let provider = LodestoneProvider()
CMIOExtensionProvider.startService(provider: provider)
CFRunLoopRun()
```

- [ ] **Step 4: Create Provider.swift**

Implements `CMIOExtensionProviderSource`:

```swift
class LodestoneProvider: NSObject, CMIOExtensionProviderSource {
    let device = LodestoneDevice()
    // ... required protocol methods
    // Return single device
}
```

- [ ] **Step 5: Create Device.swift**

Implements `CMIOExtensionDeviceSource`:

```swift
class LodestoneDevice: NSObject, CMIOExtensionDeviceSource {
    let stream = LodestoneStream()
    // ... required protocol methods
    // Return single stream
}
```

- [ ] **Step 6: Create Stream.swift — the frame delivery core**

Implements `CMIOExtensionStreamSource`. This is where frames are read from the IOSurface:

```swift
class LodestoneStream: NSObject, CMIOExtensionStreamSource {
    private var surface: IOSurfaceRef?
    private var lastSurfaceID: UInt32 = 0
    private var lastFrameCounter: UInt64 = 0

    // On startStream: look up IOSurfaceID from App Group UserDefaults
    // On timer tick (30fps):
    //   1. Read IOSurfaceID from UserDefaults (check if changed)
    //   2. IOSurfaceLookup(id) if needed
    //   3. IOSurfaceLock(surface, kIOSurfaceLockReadOnly)
    //   4. Check frame counter — if same as last, deliver previous buffer
    //   5. Read pixel data, create CVPixelBuffer, wrap in CMSampleBuffer
    //   6. IOSurfaceUnlock(surface, kIOSurfaceLockReadOnly)
    //   7. stream.send(sampleBuffer, ...)
    // On stopStream: release surface reference
}
```

- [ ] **Step 7: Create Info.plist**

Extension metadata including the `CMIOExtensionMachServiceName` and bundle identifier.

- [ ] **Step 8: Create LodestoneCamera.entitlements**

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "...">
<plist version="1.0">
<dict>
    <key>com.apple.developer.cmio.system-extension</key>
    <true/>
    <key>com.apple.security.application-groups</key>
    <array>
        <string>group.com.lodestone.app</string>
    </array>
</dict>
</plist>
```

- [ ] **Step 9: Build the extension**

Run: `cd lodestone-camera-extension && xcodebuild -scheme LodestoneCamera build`
Expected: Extension `.systemextension` bundle compiles and is code-signed (development signing).

- [ ] **Step 10: Commit**

```bash
git add lodestone-camera-extension/
git commit -m "feat: add Lodestone Camera Extension (Swift CMIOExtension)"
```

---

## Task 6: Extension installation and end-to-end testing

**Files:**
- Modify: `src/ui/toolbar.rs` (activation flow)
- Possibly modify: `src/main.rs`

**Important context**: Camera Extensions are system extensions. They must be installed via `OSSystemExtensionManager.submitRequest()` on first use, which triggers a macOS user approval prompt. The main app needs the `com.apple.developer.system-extension.install` entitlement.

- [ ] **Step 1: Add main app entitlements**

Create `Lodestone.entitlements` at the repo root with:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.system-extension.install</key>
    <true/>
    <key>com.apple.security.application-groups</key>
    <array>
        <string>group.com.lodestone.app</string>
    </array>
</dict>
</plist>
```

- [ ] **Step 2: Add extension activation to the virtual camera toggle**

When the user clicks "Virtual Camera" for the first time, call `OSSystemExtensionManager.shared.submitRequest()` via ObjC interop to install the extension. This triggers a macOS approval prompt. Cache the approval state so subsequent toggles just start/stop frame delivery.

For development without an app bundle: use `systemextensionsctl developer on` to enable developer mode, then manually install the extension with `systemextensionsctl install`. Document this in a `lodestone-camera-extension/README.md`.

- [ ] **Step 3: Install the Camera Extension for testing**

For development: enable system extension developer mode and install:
```bash
sudo systemextensionsctl developer on
# Then install the extension (exact path depends on build output)
```

- [ ] **Step 2: End-to-end test — verify camera appears**

Run: `ffplay -f avfoundation -i "Lodestone Virtual Camera"` or open Photo Booth and look for the camera in the device list.

Expected: "Lodestone Virtual Camera" appears as an available camera.

- [ ] **Step 3: End-to-end test — verify frame delivery**

1. Run Lodestone (`cargo run`)
2. Add a Display or Camera source to a scene
3. Click the Virtual Camera toggle button
4. Open Photo Booth / QuickTime Player, select "Lodestone Virtual Camera"

Expected: The composited output from Lodestone appears in the consuming app.

- [ ] **Step 4: End-to-end test — verify toggle**

Turn virtual camera off in Lodestone toolbar. Confirm consuming app sees black/frozen frame.
Turn it back on. Confirm video resumes.

- [ ] **Step 5: End-to-end test — verify without Lodestone**

Close Lodestone entirely. Confirm the camera still appears in device lists but shows black.

- [ ] **Step 6: Commit any fixes**

```bash
git add -A
git commit -m "fix: address issues found during virtual camera end-to-end testing"
```

---

## Verification

After all tasks:

- [ ] **Rust build**: `cargo build && cargo clippy && cargo test && cargo fmt --check`
- [ ] **Extension build**: `cd lodestone-camera-extension && xcodebuild -scheme LodestoneCamera build`
- [ ] **Camera visible**: "Lodestone Virtual Camera" appears in system camera list
- [ ] **Frames delivered**: Composited output visible in Photo Booth when virtual camera is on
- [ ] **Toggle works**: On/off button immediately starts/stops frame delivery
- [ ] **Clean shutdown**: Closing Lodestone doesn't crash the extension
