# Camera Extension Build & Bundle Infrastructure

**Date**: 2026-03-26

## Problem

The camera extension Swift code and IOSurface IPC are implemented, but there's no way to build the extension into a `.systemextension` bundle or package it into a launchable `.app`. Developers must manually create an Xcode project and hand-assemble the bundle — this needs to be automated.

## Solution

Three pieces of infrastructure:

1. An Xcode project to compile the Swift code into a `.systemextension` bundle
2. A `build.rs` that calls `xcodebuild` when Swift sources change
3. A `scripts/bundle.sh` that assembles the final `.app` with the Rust binary + embedded extension
4. Rust-side extension activation via `OSSystemExtensionManager`

## 1. Xcode Project

**Location**: `lodestone-camera-extension/LodestoneCamera.xcodeproj`

**Single target** with these settings:

| Setting | Value |
|---------|-------|
| Product Name | `LodestoneCamera` |
| Product Type | `com.apple.product-type.system-extension` |
| Bundle Identifier | `com.lodestone.camera-extension` |
| `PRODUCT_NAME` | `LodestoneCamera` (matches `CFBundleExecutable` in Info.plist) |
| `PRODUCT_MODULE_NAME` | `LodestoneCamera` (so `NSExtensionPrincipalClass` resolves to `LodestoneCamera.LodestoneProvider`) |
| Deployment Target | macOS 13.0 |
| Swift Language Version | 5 |
| Signing | Automatic, Team ID `9YR7S3S6AS` |
| Entitlements | `LodestoneCamera.entitlements` |
| Info.plist | Existing `Info.plist` in the extension directory |

**Sources**: The 4 existing files in `Sources/`:
- `main.swift`
- `Provider.swift`
- `Device.swift`
- `Stream.swift`

**Frameworks**: `CoreMediaIO`, `CoreVideo`, `IOSurface`, `Foundation` (all system frameworks).

The `.xcodeproj` is committed to git. It rarely changes since the Swift source list is stable.

## 2. Build Integration (`build.rs`)

A new `build.rs` at the repo root.

**Behavior**:

```
cargo build
  └── build.rs executes
        ├── cargo:rerun-if-changed=lodestone-camera-extension/Sources/main.swift
        ├── cargo:rerun-if-changed=lodestone-camera-extension/Sources/Provider.swift
        ├── cargo:rerun-if-changed=lodestone-camera-extension/Sources/Device.swift
        ├── cargo:rerun-if-changed=lodestone-camera-extension/Sources/Stream.swift
        ├── cargo:rerun-if-changed=lodestone-camera-extension/Info.plist
        ├── cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.entitlements
        └── xcodebuild -project lodestone-camera-extension/LodestoneCamera.xcodeproj
                        -scheme LodestoneCamera
                        -configuration {Debug|Release}
                        -derivedDataPath target/xcode-build
                        DEVELOPMENT_TEAM=9YR7S3S6AS
                        build
```

**Configuration mapping**: `build.rs` reads the `PROFILE` env var — maps `release` to Xcode `Release`, everything else (including `debug` and custom profiles) to Xcode `Debug`.

**Output location**: `target/xcode-build/Build/Products/{Debug|Release}/com.lodestone.camera-extension.systemextension/`

**Also watch the project file**: `cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.xcodeproj/project.pbxproj`

**macOS-only**: The entire `build.rs` body is gated on `#[cfg(target_os = "macos")]`. On other platforms it's a no-op.

**Prerequisite check**: `build.rs` should verify that `xcode-select -p` points to an Xcode.app installation (not just CommandLineTools). If it doesn't, emit a helpful error: `"xcodebuild requires Xcode. Run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer"`.

**Error handling**: If `xcodebuild` fails, `build.rs` panics with the full stderr so the developer sees what went wrong.

## 3. App Bundle Assembly (`scripts/bundle.sh`)

A shell script that assembles the `.app` bundle.

**Usage**:
```bash
cargo build --release && ./scripts/bundle.sh          # release
cargo build && ./scripts/bundle.sh --debug             # debug
```

**What it does**:

```bash
# 1. Create bundle structure
mkdir -p Lodestone.app/Contents/{MacOS,Resources,Library/SystemExtensions}

# 2. Generate host Info.plist
cat > Lodestone.app/Contents/Info.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist ...>
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.lodestone.app</string>
    <key>CFBundleName</key>
    <string>Lodestone</string>
    <key>CFBundleExecutable</key>
    <string>lodestone</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSCameraUsageDescription</key>
    <string>Lodestone needs camera access for video capture.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Lodestone needs microphone access for audio capture.</string>
    <key>NSSystemExtensionUsageDescription</key>
    <string>Lodestone installs a virtual camera so other apps can use your composited output.</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

# 3. Copy Rust binary
cp target/{release|debug}/lodestone Lodestone.app/Contents/MacOS/

# 4. Copy .systemextension
cp -R target/xcode-build/Build/Products/{Release|Debug}/com.lodestone.camera-extension.systemextension \
      Lodestone.app/Contents/Library/SystemExtensions/

# 5. Sign the extension first (inner), then the app (outer)
codesign --force --sign "Apple Development" \
    --entitlements lodestone-camera-extension/LodestoneCamera.entitlements \
    --options runtime \
    Lodestone.app/Contents/Library/SystemExtensions/com.lodestone.camera-extension.systemextension

codesign --force --sign "Apple Development" \
    --entitlements Lodestone.entitlements \
    --options runtime \
    Lodestone.app
```

**Signing order matters**: inner bundles (extension) must be signed before outer bundles (app).

## 4. Extension Activation (Rust)

On app startup, Lodestone programmatically requests the system to activate the camera extension.

**Location**: New function in a `src/system_extension.rs` module (macOS-only).

**Flow**:
1. Call `OSSystemExtensionManager.shared().submitRequest()` via `objc2`
2. The request includes the extension's bundle identifier (`com.lodestone.camera-extension`)
3. Implement `OSSystemExtensionRequestDelegate` to handle:
   - `.completed` — extension is active, log success
   - `.willCompleteAfterReboot` — log, inform user
   - `.failed(error)` — log error, surface to user
   - `.requestNeedsUserApproval` — macOS shows its own system dialog
4. On subsequent launches, if the extension is already registered, the submit call is a no-op

**First-run UX**: macOS will show a system prompt asking the user to approve the extension in System Settings → Privacy & Security. This is mandatory and cannot be bypassed. The app should show a brief message like "Approve the Lodestone camera extension in System Settings to enable virtual camera."

**Dependencies**: Raw `objc2` FFI to `SystemExtensions.framework`, consistent with the project's existing `objc2` usage pattern (no wrapper crate needed).

## 5. File Summary

**New files**:
- `lodestone-camera-extension/LodestoneCamera.xcodeproj/` — Xcode project
- `build.rs` — calls xcodebuild
- `scripts/bundle.sh` — assembles .app bundle
- `src/system_extension.rs` — extension activation via objc2 (macOS-only)

**Modified files**:
- `Cargo.toml` — add `build = "build.rs"`
- `src/main.rs` — call `system_extension::activate()` on startup

**Unchanged**:
- All existing Swift files in `lodestone-camera-extension/Sources/`
- `lodestone-camera-extension/Info.plist`
- `lodestone-camera-extension/LodestoneCamera.entitlements`
- `Lodestone.entitlements`
- `src/gstreamer/virtual_camera.rs`

## 6. Testing

1. **Build**: `cargo build` succeeds and produces the `.systemextension` in `target/xcode-build/`
2. **Bundle**: `./scripts/bundle.sh --debug` creates a valid `Lodestone.app` with correct structure
3. **Signing**: `codesign --verify --deep Lodestone.app` passes
4. **Launch**: Double-click `Lodestone.app`, confirm it launches and requests extension activation
5. **Camera visible**: After approval, `Lodestone Virtual Camera` appears in Photo Booth / QuickTime / `ffplay`
6. **Rebuild**: Change a Swift file, run `cargo build`, confirm xcodebuild runs. Change a Rust file only, confirm xcodebuild is skipped.

## Constraints

- macOS 13.0+ (CMIOExtension + SystemExtensions framework)
- Xcode must be installed (for `xcodebuild`)
- Apple Developer Program membership required for entitlements
- First-run requires user approval in System Settings (OS-enforced, cannot be bypassed)
- Extension and app must share the same App Group (`group.com.lodestone.app`)
- Signing identity: `Apple Development` (auto-selects from keychain)
- Development Team: `9YR7S3S6AS`
