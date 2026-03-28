# Virtual Camera DAL Plugin

**Date**: 2026-03-27

## Problem

The CMIOExtension system extension approach for the virtual camera requires restricted Apple entitlements and SIP disabled for development. This is impractical. The legacy CoreMediaIO DAL plugin approach works without any entitlements, SIP changes, or system extension registration.

## Solution

Replace the CMIOExtension system extension with a CoreMediaIO DAL plugin. The plugin is a `.plugin` bundle installed to `/Library/CoreMediaIO/Plug-Ins/DAL/`. It implements the `CMIOHardwarePlugIn` C interface, reads composited frames from the existing IOSurface via App Group UserDefaults, and presents as "Lodestone Virtual Camera" to any app using AVFoundation.

```
Lodestone (Rust) → IOSurface + UserDefaults → DAL Plugin (ObjC) → consuming apps
```

The Rust-side IPC (`src/gstreamer/virtual_camera.rs`) is unchanged.

## Plugin Bundle Structure

```
LodestoneCamera.plugin/
├── Contents/
│   ├── Info.plist
│   └── MacOS/
│       └── LodestoneCamera
```

**Info.plist keys:**
- `CFBundleIdentifier`: `com.lodestone.app.camera-dal`
- `CFBundleExecutable`: `LodestoneCamera`
- `CMIOHardwarePlugInFactoryFunction`: `LodestoneCameraPlugInCreate`

## Implementation

Single ObjC file: `lodestone-camera-dal/LodestoneCamera.m` (~300-400 lines).

### CMIOHardwarePlugIn Interface

The plugin exports a factory function `LodestoneCameraPlugInCreate` that returns a `CMIOHardwarePlugInRef` — a pointer to a struct of C function pointers implementing the plugin interface.

**Required functions:**
- `InitializeWithObjectID` — Register the device + stream with CoreMediaIO
- `Teardown` — Clean up resources
- `ObjectShow` / `ObjectHasProperty` / `ObjectIsPropertySettable` / `ObjectGetPropertyDataSize` / `ObjectGetPropertyData` / `ObjectSetPropertyData` — Property access for device, stream, and plugin objects
- `StreamCopyBufferQueue` — Provide the `CMSimpleQueue` that frames are delivered through

### Objects

Three CoreMediaIO objects, each with an `ObjectID` assigned by the system:

1. **Plugin** — Top-level object. Properties: manufacturer name.
2. **Device** — "Lodestone Virtual Camera". Properties: name, model, transport type, streams list.
3. **Stream** — Single output stream. Properties: format description (BGRA), frame rate, active format. Owns the frame delivery queue.

### Frame Delivery

- On `InitializeWithObjectID`, start a `dispatch_source` timer at the configured FPS
- Each tick: read IOSurface ID from `NSUserDefaults(suiteName: "group.com.lodestone.app")`, call `IOSurfaceLookup`, wrap in `CVPixelBuffer` + `CMSampleBuffer`, enqueue to `CMSimpleQueue`
- If no IOSurface available (Lodestone not running): deliver a black frame
- IOSurface locking: `IOSurfaceLock` with read-only flag before reading, unlock after

### UserDefaults Keys

Same as existing (matches `src/gstreamer/virtual_camera.rs`):
- `virtualCameraSurfaceID` (UInt32)
- `virtualCameraWidth` (UInt32)
- `virtualCameraHeight` (UInt32)
- `virtualCameraFPS` (UInt32)

## Build Integration

`build.rs` compiles the ObjC file using the `cc` crate:

```rust
cc::Build::new()
    .file("lodestone-camera-dal/LodestoneCamera.m")
    .flag("-fobjc-arc")
    .flag("-framework").flag("CoreMediaIO")
    .flag("-framework").flag("CoreMedia")
    .flag("-framework").flag("CoreVideo")
    .flag("-framework").flag("IOSurface")
    .flag("-framework").flag("Foundation")
    .compile("LodestoneCamera");
```

Then `build.rs` assembles the `.plugin` bundle structure in `target/{debug|release}/LodestoneCamera.plugin/`.

No Xcode project needed. The `cc` crate handles compilation.

**Watched files:**
- `cargo:rerun-if-changed=lodestone-camera-dal/LodestoneCamera.m`
- `cargo:rerun-if-changed=lodestone-camera-dal/Info.plist`

## Installation

`scripts/bundle.sh` copies the plugin to `/Library/CoreMediaIO/Plug-Ins/DAL/LodestoneCamera.plugin/`. This requires `sudo` and only needs to happen once (or when the plugin binary changes).

The script detects if the installed plugin is outdated and prompts for reinstall.

## Files

**New:**
- `lodestone-camera-dal/LodestoneCamera.m` — DAL plugin implementation (ObjC)
- `lodestone-camera-dal/Info.plist` — Plugin bundle metadata

**Modified:**
- `build.rs` — Replace xcodebuild with `cc` crate compilation
- `scripts/bundle.sh` — Add DAL plugin install step
- `Cargo.toml` — Add `cc` build dependency

**Removed:**
- `lodestone-camera-extension/` — Entire Swift CMIOExtension directory (Sources/, xcodeproj, entitlements, Info.plist, README)

**Unchanged:**
- `src/gstreamer/virtual_camera.rs` — IOSurface writer (Rust side)
- `Lodestone.entitlements` — App group entitlement stays for UserDefaults sharing

## Testing

1. `cargo build` compiles the plugin
2. `./scripts/bundle.sh --debug` builds the app bundle and installs the DAL plugin
3. Open Photo Booth → "Lodestone Virtual Camera" should appear in Camera menu
4. Enable virtual camera in Lodestone → frames should appear in Photo Booth
5. Close Lodestone → Photo Booth should show black frames
6. `ffplay -f avfoundation -i "Lodestone Virtual Camera"` as CLI verification

## Constraints

- macOS only (DAL plugins are macOS-specific)
- Deprecated API — Apple deprecated DAL plugins in favor of CMIOExtension. They still work on macOS 26 but may eventually be removed. When Apple makes CMIOExtension app extensions viable (no restricted entitlements), migrate back.
- Installation requires `sudo` once to copy to `/Library/CoreMediaIO/Plug-Ins/DAL/`
- Plugin runs in the address space of the consuming app (Zoom, Discord, etc.), not sandboxed
