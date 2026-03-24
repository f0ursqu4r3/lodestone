# Virtual Camera Output

**Date**: 2026-03-24

## Problem

Users want to use Lodestone's composited output as a camera source in other apps (Zoom, Discord, FaceTime, etc.). This requires registering a virtual camera device with the OS.

## Solution

Two binaries: the main Lodestone app writes composited frames to shared memory, and a platform-specific camera driver reads them and presents as a system camera.

This spec covers macOS. Windows (DirectShow/MF) and Linux (v4l2loopback) follow the same shared-memory IPC pattern but need their own driver binaries.

## Architecture

```
Compositor → GPU readback → RgbaFrame → IOSurface (shared memory) → Camera Extension → consuming app
```

### 1. Camera Extension Binary

**Directory**: `lodestone-camera-extension/` at the repo root.

**Language**: Swift (~200 lines). CMIOExtensionProvider APIs are heavily designed around Swift/ObjC patterns. The extension is a tiny binary that reads from shared memory — fighting `objc2` for extension lifecycle management adds complexity with no upside. The main Lodestone app stays 100% Rust.

**What it registers**:
- **Device**: "Lodestone Virtual Camera"
- **Stream**: single video stream, BGRA pixel format, fixed resolution (see Resolution Handling below)
- **Source**: reads from an `IOSurface` shared via App Group container

**Protocols implemented**: `CMIOExtensionProviderSource`, `CMIOExtensionDeviceSource`, `CMIOExtensionStreamSource`.

**Packaging**: `.systemextension` bundle embedded in the Lodestone app bundle.

**Behavior when Lodestone isn't running**: delivers a black frame.

### 2. Extension Installation and Activation

Camera Extensions are system extensions that require explicit user approval.

**First-run activation flow**:
1. User clicks "Virtual Camera" toggle in Lodestone toolbar
2. Lodestone calls `OSSystemExtensionManager.shared.submitRequest()` via ObjC interop to install the extension
3. macOS prompts the user to approve the system extension (System Settings → Privacy & Security)
4. Once approved, the extension is loaded by the system and appears as a camera in all apps
5. Subsequent toggles just start/stop frame delivery — no re-approval needed

**Required entitlements**:
- **Main app**: `com.apple.developer.system-extension.install` — permission to install system extensions
- **Camera Extension**: `com.apple.developer.cmio.system-extension` — registers as a CoreMediaIO device (requires Apple provisioning profile)
- **Both**: `com.apple.security.application-groups` with a shared group ID (e.g., `group.com.lodestone.app`) — for IOSurface sharing via the App Group container

**Code signing**: Both the main app and extension must be signed. Development signing works for local testing; distribution requires a Developer ID certificate and notarization.

### 3. IPC — Shared Memory Frame Delivery

**IOSurface** for zero-copy frame sharing between processes.

**Surface sharing mechanism**: The main app creates an `IOSurface`, gets its `IOSurfaceID` (uint32 global token via `IOSurfaceGetID()`), and writes the ID to the App Group's `UserDefaults(suiteName: "group.com.lodestone.app")` under a known key. The extension reads this ID and calls `IOSurfaceLookup(surfaceID)` to get a reference.

**From Lodestone** (Rust, `src/gstreamer/virtual_camera.rs`):
- Create an `IOSurface` at virtual camera start (allocated at max supported resolution, e.g., 1920x1080)
- Publish the `IOSurfaceID` to App Group UserDefaults
- Each frame: `IOSurfaceLock(surface, 0)` (write lock), memcpy BGRA data, `IOSurfaceUnlock(surface, 0)`, increment an atomic frame counter stored in the IOSurface seed value (`IOSurfaceSetValue` or a shared metadata region)

**From the extension** (Swift):
- On init / when notified: look up the `IOSurfaceID` from App Group UserDefaults, call `IOSurfaceLookup()`
- Poll on the `CMIOExtensionStream`'s configured frame rate timer (e.g., 30fps). Each tick: `IOSurfaceLock(surface, kIOSurfaceLockReadOnly)`, check frame counter, read pixel data into `CMSampleBuffer`, `IOSurfaceUnlock()`, deliver to stream
- No Darwin notifications needed — the extension polls at its own cadence, same approach as OBS

**Performance**: IOSurface is GPU-backed shared memory. No serialization, no cross-process copies. Polling at 30fps is negligible overhead.

### 4. Resolution Handling

**Fixed IOSurface size**: Allocate the IOSurface at the maximum output resolution (e.g., 1920x1080). Always write scaled frames to fit within this surface. Never recreate the IOSurface — this avoids race conditions and avoids triggering format changes in consuming apps.

The Camera Extension advertises this fixed resolution. If Lodestone's output resolution is smaller, it writes into the top-left portion of the surface and the metadata region stores the active `width` and `height`. The extension reads only the active region.

**Why fixed**: Most webcam-consuming apps (Zoom, Discord, FaceTime) negotiate a format at stream start and do not handle dynamic resolution changes gracefully. A fixed surface with variable active region is simpler and more compatible.

If the user changes their output resolution to something larger than the pre-allocated surface, the surface is recreated (rare edge case). The extension detects the stale ID via the frame counter and re-reads the new ID from UserDefaults.

### 5. Virtual Camera Module

**New file**: `src/gstreamer/virtual_camera.rs` — macOS-only (`#[cfg(target_os = "macos")]`).

**Responsibilities**:
- Create/destroy the IOSurface
- Publish IOSurfaceID to App Group UserDefaults
- Write composited frames with proper locking (RGBA→BGRA conversion + memcpy)
- Maintain frame counter for the extension to detect new frames
- Expose start/stop API

**Public API**:
```rust
pub struct VirtualCameraHandle { /* IOSurface + metadata */ }

pub fn start_virtual_camera(width: u32, height: u32, fps: u32) -> Result<VirtualCameraHandle>
pub fn write_frame(handle: &VirtualCameraHandle, frame: &RgbaFrame) -> Result<()>
pub fn stop_virtual_camera(handle: VirtualCameraHandle) -> Result<()>
```

**RGBA→BGRA conversion**: Byte swap on every frame. Alternatively, the compositor's readback texture format could be changed to `Bgra8Unorm` (universally supported by wgpu) to avoid the conversion entirely. This optimization should be evaluated during implementation.

**Error handling**: `write_frame` errors are logged (not silently dropped), following the project convention.

**Lock semantics**: Every write is bracketed by `IOSurfaceLock(surface, 0)` / `IOSurfaceUnlock(surface, 0)` (write lock). The extension uses `kIOSurfaceLockReadOnly` for reads. This prevents torn frames.

### 6. App Integration

**GStreamer thread** (`src/gstreamer/thread.rs`): In the composited frame loop (~line 798), add virtual camera as a third sink:

```rust
while let Ok(frame) = self.channels.composited_frame_rx.try_recv() {
    // existing: push to stream pipeline
    // existing: push to record pipeline
    // NEW: write to IOSurface for virtual camera
    if let Some(ref vc) = self.virtual_camera_handle {
        if let Err(e) = virtual_camera::write_frame(vc, &frame) {
            log::warn!("Virtual camera frame write failed: {e}");
        }
    }
}
```

**Compositing trigger** (`src/main.rs`): GPU readback currently only happens when streaming or recording. Add virtual camera to the condition:
```rust
let is_encoding = is_streaming || is_recording || virtual_camera_active;
```

Note: When virtual camera is the only active output, this triggers GPU readback overhead solely for the virtual camera. This is acceptable.

**New GstCommands** (`src/gstreamer/commands.rs`):
- `StartVirtualCamera` — create IOSurface, publish ID, begin writing frames
- `StopVirtualCamera` — stop writing, release IOSurface, clear published ID

**State** (`src/state.rs`): Add `virtual_camera_active: bool` to `AppState`. Runtime toggle, not a persistent setting.

**UI** (`src/ui/toolbar.rs`): Add a "Virtual Camera" toggle button alongside the existing Stream and Record buttons.

### 7. Files

**New files**:
- `lodestone-camera-extension/` — Swift Camera Extension project (separate build target)
- `src/gstreamer/virtual_camera.rs` — IOSurface frame writer, macOS-only

**Modified files**:
- `Cargo.toml` — add `io-surface` crate dependency if needed (or use raw FFI)
- `src/gstreamer/commands.rs` — add StartVirtualCamera/StopVirtualCamera commands
- `src/gstreamer/thread.rs` — handle new commands, add frame routing, store VirtualCameraHandle
- `src/gstreamer/mod.rs` — register virtual_camera module (with `#[cfg(target_os = "macos")]`)
- `src/state.rs` — add virtual_camera_active field
- `src/ui/toolbar.rs` — add Virtual Camera toggle button
- `src/main.rs` — update is_encoding condition

### 8. Testing

- **Verify camera appears**: Run `ffplay -f avfoundation -i "Lodestone Virtual Camera"` or check System Information → Camera to confirm the device registers
- **Verify frame delivery**: Open Photo Booth or QuickTime Player → New Movie Recording, select "Lodestone Virtual Camera", confirm video appears
- **Verify toggle**: Turn virtual camera off in Lodestone, confirm consuming apps see black/frozen frame
- **Verify without Lodestone**: Close Lodestone, confirm the camera still appears in device lists but shows black

## Platform Note

Each platform needs its own camera driver binary:
- **macOS**: CMIOExtension (Swift, `.systemextension` bundle) — this spec
- **Windows**: DirectShow filter or Media Foundation virtual camera (C++, `.dll`) — future spec
- **Linux**: v4l2loopback kernel module + userspace writer — future spec

The shared memory IPC pattern is the portable abstraction boundary. The Rust `virtual_camera.rs` module is the platform-specific writer side; each platform's driver is the reader side.

## Constraints

- macOS 12.3+ (CMIOExtensionProvider requirement)
- Camera Extension requires `com.apple.developer.cmio.system-extension` entitlement (Apple provisioning profile)
- Main app requires `com.apple.developer.system-extension.install` entitlement
- Both require matching App Group for IOSurface sharing
- Code signing required (development signing for local testing, Developer ID for distribution)
- IOSurface locking is mandatory for safe concurrent access
- RGBA→BGRA conversion per frame (can be optimized by changing readback format to BGRA)
