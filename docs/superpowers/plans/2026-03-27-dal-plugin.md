# DAL Plugin Virtual Camera Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the CMIOExtension system extension with a CoreMediaIO DAL plugin that registers "Lodestone Virtual Camera" as a system camera device, reading frames from the existing IOSurface shared memory.

**Architecture:** A single ObjC file implements the `CMIOHardwarePlugIn` C interface. `build.rs` compiles it via `cc` crate and assembles the `.plugin` bundle. `bundle.sh` installs it to `/Library/CoreMediaIO/Plug-Ins/DAL/`. The Rust-side IOSurface writer is unchanged.

**Tech Stack:** Objective-C, CoreMediaIO DAL API, `cc` crate, IOSurface, NSUserDefaults

**Spec:** `docs/superpowers/specs/2026-03-27-dal-plugin-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `lodestone-camera-dal/LodestoneCamera.m` | Create | DAL plugin: plugin lifecycle, device, stream, frame delivery |
| `lodestone-camera-dal/Info.plist` | Create | Plugin bundle metadata |
| `build.rs` | Rewrite | Compile ObjC via `cc`, assemble `.plugin` bundle |
| `scripts/bundle.sh` | Rewrite | Build app bundle + install DAL plugin |
| `Cargo.toml` | Modify | Add `cc` build dependency |
| `lodestone-camera-extension/` | Delete | Remove entire Swift CMIOExtension directory |

---

### Task 1: Create the DAL Plugin ObjC Source

**Files:**
- Create: `lodestone-camera-dal/LodestoneCamera.m`
- Create: `lodestone-camera-dal/Info.plist`

- [ ] **Step 1: Create the directory**

```bash
mkdir -p lodestone-camera-dal
```

- [ ] **Step 2: Write `Info.plist`**

Create `lodestone-camera-dal/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.lodestone.app.camera-dal</string>
    <key>CFBundleName</key>
    <string>LodestoneCamera</string>
    <key>CFBundleExecutable</key>
    <string>LodestoneCamera</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CMIOHardwarePlugInFactoryFunction</key>
    <string>LodestoneCameraPlugInCreate</string>
</dict>
</plist>
```

- [ ] **Step 3: Write `LodestoneCamera.m`**

Create `lodestone-camera-dal/LodestoneCamera.m`. This is the core of the plugin — a single file implementing the full DAL interface.

The file implements these pieces:

**A) Global state** — Object IDs for plugin, device, and stream. The `CMSimpleQueue` for frame delivery. The IOSurface pointer and timer.

**B) Factory function** — `LodestoneCameraPlugInCreate()` returns a `CMIOHardwarePlugInRef` (pointer to the vtable of function pointers).

**C) Plugin interface functions:**
- `HardwarePlugIn_InitializeWithObjectID` — Called by CoreMediaIO when the plugin loads. Creates the device and stream objects by calling `CMIOObjectCreate`, and sends `kCMIOObjectPropertyOwnedObjects` changed notifications.
- `HardwarePlugIn_Teardown` — Stops timer, releases IOSurface, cleans up.
- `HardwarePlugIn_ObjectShow` — No-op.
- `HardwarePlugIn_ObjectHasProperty` / `ObjectIsPropertySettable` / `ObjectGetPropertyDataSize` / `ObjectGetPropertyData` / `ObjectSetPropertyData` — Property dispatch. Routes to plugin/device/stream handlers based on object ID.
- `HardwarePlugIn_StreamCopyBufferQueue` — Returns the `CMSimpleQueue` for the stream. The `queueAlteredProc` callback is stored and called after each frame enqueue.

**D) Property handling:**
- **Plugin**: `kCMIOObjectPropertyOwnedObjects` (returns device ID)
- **Device**: `kCMIOObjectPropertyName` ("Lodestone Virtual Camera"), `kCMIODevicePropertyDeviceUID` (unique string), `kCMIOObjectPropertyOwnedObjects` (returns stream ID), `kCMIODevicePropertyStreams`, `kCMIODevicePropertyModelUID`, `kCMIODevicePropertyTransportType`
- **Stream**: `kCMIOStreamPropertyDirection` (source=0), `kCMIOStreamPropertyFormatDescription` (BGRA video format), `kCMIOStreamPropertyFrameRate` / `kCMIOStreamPropertyFrameRates`, `kCMIOStreamPropertyMinimumFrameRate`

**E) Frame delivery:**
- `startStream()` — Start a `dispatch_source` timer at 30fps (or configured fps from UserDefaults key `virtualCameraFPS`, default 30).
- Each tick: read surface ID from `NSUserDefaults(suiteName: "group.com.lodestone.app")` key `virtualCameraSurfaceID`. If non-zero, `IOSurfaceLookup`, `IOSurfaceLock` read-only, `CVPixelBufferCreateWithIOSurface`, build `CMSampleBuffer`, enqueue to `CMSimpleQueue`, call `queueAlteredProc`. If zero or lookup fails, deliver a black frame.
- `stopStream()` — Cancel timer, release surface.

**Key constants** (must match `src/gstreamer/virtual_camera.rs`):
```objc
static NSString *const kAppGroupSuite = @"group.com.lodestone.app";
static NSString *const kSurfaceIDKey  = @"virtualCameraSurfaceID";
static NSString *const kWidthKey      = @"virtualCameraWidth";
static NSString *const kHeightKey     = @"virtualCameraHeight";
static NSString *const kFPSKey        = @"virtualCameraFPS";
```

**Default resolution**: 1920x1080 if UserDefaults has no values yet.

The full ObjC implementation should be written as a single self-contained file. Reference OBS's `OBSDALPlugIn` and `obs-mac-virtualcam` for patterns, but keep it minimal — we only need one device with one stream.

- [ ] **Step 4: Verify the file compiles standalone**

```bash
clang -c -fobjc-arc \
    -framework CoreMediaIO -framework CoreMedia -framework CoreVideo \
    -framework IOSurface -framework Foundation \
    lodestone-camera-dal/LodestoneCamera.m \
    -o /tmp/LodestoneCamera.o
```

Expected: compiles without errors.

```bash
rm /tmp/LodestoneCamera.o
```

- [ ] **Step 5: Commit**

```bash
git add lodestone-camera-dal/
git commit -m "feat: add CoreMediaIO DAL plugin ObjC source"
```

---

### Task 2: Rewrite `build.rs` for DAL Plugin

**Files:**
- Modify: `build.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `cc` build dependency to `Cargo.toml`**

Add to `Cargo.toml` after `[dev-dependencies]`:

```toml
[build-dependencies]
cc = "1"
```

- [ ] **Step 2: Rewrite `build.rs`**

Replace the entire `build.rs` content. The new version:

1. Gates on `CARGO_CFG_TARGET_OS == "macos"`
2. Sets `cargo:rerun-if-changed` for `lodestone-camera-dal/LodestoneCamera.m` and `lodestone-camera-dal/Info.plist`
3. Compiles `LodestoneCamera.m` into a dynamic library using `cc::Build` — but since `cc` produces static libs and we need a `.dylib` (for a plugin bundle), use `std::process::Command` to call `clang` directly:

```rust
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    println!("cargo:rerun-if-changed=lodestone-camera-dal/LodestoneCamera.m");
    println!("cargo:rerun-if-changed=lodestone-camera-dal/Info.plist");

    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Target directory: target/{debug|release}/LodestoneCamera.plugin/Contents/MacOS/
    let target_dir = Path::new("target").join(&profile);
    let plugin_dir = target_dir.join("LodestoneCamera.plugin");
    let contents_dir = plugin_dir.join("Contents");
    let macos_dir = contents_dir.join("MacOS");

    fs::create_dir_all(&macos_dir).expect("failed to create plugin bundle directories");

    // Copy Info.plist
    fs::copy(
        "lodestone-camera-dal/Info.plist",
        contents_dir.join("Info.plist"),
    )
    .expect("failed to copy Info.plist");

    // Compile ObjC -> dynamic library
    let dylib_path = macos_dir.join("LodestoneCamera");
    let status = Command::new("clang")
        .args([
            "-dynamiclib",
            "-fobjc-arc",
            "-o",
        ])
        .arg(&dylib_path)
        .arg("lodestone-camera-dal/LodestoneCamera.m")
        .args([
            "-framework", "CoreMediaIO",
            "-framework", "CoreMedia",
            "-framework", "CoreVideo",
            "-framework", "IOSurface",
            "-framework", "Foundation",
            "-framework", "CoreFoundation",
        ])
        .status()
        .expect("failed to run clang");

    if !status.success() {
        panic!("clang failed to compile DAL plugin");
    }
}
```

Note: we don't actually need the `cc` crate since we're calling `clang` directly for the dylib. But having it as a build-dep doesn't hurt and could be useful later. Alternatively, just remove it from `Cargo.toml` if not needed.

- [ ] **Step 3: Build and verify**

```bash
cargo build
```

Expected: compiles Rust + DAL plugin. Verify:

```bash
ls target/debug/LodestoneCamera.plugin/Contents/MacOS/LodestoneCamera
ls target/debug/LodestoneCamera.plugin/Contents/Info.plist
file target/debug/LodestoneCamera.plugin/Contents/MacOS/LodestoneCamera
```

Expected: `LodestoneCamera` is a Mach-O dynamic library.

- [ ] **Step 4: Commit**

```bash
git add build.rs Cargo.toml
git commit -m "feat: rewrite build.rs to compile DAL plugin via clang"
```

---

### Task 3: Update `bundle.sh` and Remove Old Extension

**Files:**
- Modify: `scripts/bundle.sh`
- Delete: `lodestone-camera-extension/` (entire directory)

- [ ] **Step 1: Rewrite `scripts/bundle.sh`**

Replace the entire script. The new version:

1. Accepts `--debug` flag (default release)
2. Verifies the Rust binary exists
3. Verifies the DAL plugin bundle exists at `target/{debug|release}/LodestoneCamera.plugin/`
4. Assembles `Lodestone.app`:
   - Creates `Contents/{MacOS,Resources}`
   - Generates `Info.plist` (same as before but without `NSSystemExtensionUsageDescription`)
   - Copies Rust binary
   - Signs the app bundle
5. Installs DAL plugin:
   - Copies `LodestoneCamera.plugin` to `/Library/CoreMediaIO/Plug-Ins/DAL/`
   - This step requires `sudo` — the script prompts for it
   - Skips if the installed plugin is identical (compare with `diff -r`)

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_MODE="release"
SIGN_IDENTITY="Apple Development"

for arg in "$@"; do
    case "$arg" in
        --debug) BUILD_MODE="debug" ;;
        *) echo "Usage: $0 [--debug]" >&2; exit 1 ;;
    esac
done

# ─── Paths ────────────────────────────────────────────────────────────────────

RUST_BINARY="${REPO_ROOT}/target/${BUILD_MODE}/lodestone"
PLUGIN_SRC="${REPO_ROOT}/target/${BUILD_MODE}/LodestoneCamera.plugin"
APP_ENTITLEMENTS="${REPO_ROOT}/Lodestone.entitlements"
APP_BUNDLE="${REPO_ROOT}/Lodestone.app"
DAL_DEST="/Library/CoreMediaIO/Plug-Ins/DAL/LodestoneCamera.plugin"

# ─── Pre-flight ───────────────────────────────────────────────────────────────

[[ -f "$RUST_BINARY" ]] || { echo "error: Rust binary not found. Run cargo build first." >&2; exit 1; }
[[ -d "$PLUGIN_SRC" ]] || { echo "error: DAL plugin not found at $PLUGIN_SRC" >&2; exit 1; }

echo "Building ${BUILD_MODE} app bundle..."

# ─── App bundle ───────────────────────────────────────────────────────────────

rm -rf "$APP_BUNDLE"
mkdir -p "${APP_BUNDLE}/Contents/MacOS" "${APP_BUNDLE}/Contents/Resources"

cat > "${APP_BUNDLE}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
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
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSCameraUsageDescription</key>
    <string>Lodestone needs camera access to capture video for streaming and recording.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Lodestone needs microphone access to capture audio for streaming and recording.</string>
</dict>
</plist>
PLIST

cp "$RUST_BINARY" "${APP_BUNDLE}/Contents/MacOS/lodestone"

echo "Signing app bundle..."
codesign --force --sign "$SIGN_IDENTITY" \
    --entitlements "$APP_ENTITLEMENTS" \
    --options runtime --timestamp "$APP_BUNDLE"

codesign --verify --deep --strict "$APP_BUNDLE"

echo "App bundle: ${APP_BUNDLE}"

# ─── DAL plugin install ──────────────────────────────────────────────────────

if diff -r "$PLUGIN_SRC" "$DAL_DEST" &>/dev/null 2>&1; then
    echo "DAL plugin already installed and up to date."
else
    echo ""
    echo "Installing DAL plugin to ${DAL_DEST} (requires sudo)..."
    sudo mkdir -p /Library/CoreMediaIO/Plug-Ins/DAL
    sudo rm -rf "$DAL_DEST"
    sudo cp -R "$PLUGIN_SRC" "$DAL_DEST"
    echo "DAL plugin installed. Restart any apps using cameras to pick it up."
fi

echo ""
echo "Done! Build mode: ${BUILD_MODE}"
```

- [ ] **Step 2: Remove the old CMIOExtension directory**

```bash
rm -rf lodestone-camera-extension/
```

- [ ] **Step 3: Test the full pipeline**

```bash
cargo build && ./scripts/bundle.sh --debug
```

Expected:
- Rust compiles
- DAL plugin compiles via clang
- App bundle is assembled and signed
- DAL plugin is installed to `/Library/CoreMediaIO/Plug-Ins/DAL/`
- "Lodestone Virtual Camera" appears in Photo Booth's Camera menu

- [ ] **Step 4: Verify the virtual camera appears**

Open Photo Booth (or QuickTime Player → New Movie Recording) and check the Camera menu for "Lodestone Virtual Camera".

Alternative CLI check:
```bash
ffplay -f avfoundation -list_devices true -i "" 2>&1 | grep -i lodestone
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: switch to DAL plugin, remove CMIOExtension system extension"
```

---

### Task 4: End-to-End Verification

**Files:** None (testing only)

- [ ] **Step 1: Clean build**

```bash
cargo clean
cargo build && ./scripts/bundle.sh --debug
```

Expected: full rebuild succeeds.

- [ ] **Step 2: Verify camera appears in Photo Booth**

1. Open Photo Booth
2. Camera menu → "Lodestone Virtual Camera" should be listed
3. Select it → should show black frames (Lodestone virtual camera not enabled yet)

- [ ] **Step 3: Verify frame delivery**

1. Launch Lodestone app: `open Lodestone.app` or `cargo run`
2. Enable virtual camera toggle in the toolbar
3. Switch to Photo Booth → should show the composited output from Lodestone

- [ ] **Step 4: Verify rebuild skipping**

```bash
touch src/main.rs
cargo build 2>&1
```

Confirm clang is NOT re-run (only Rust recompiles).

```bash
touch lodestone-camera-dal/LodestoneCamera.m
cargo build 2>&1
```

Confirm clang IS re-run.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix: adjustments from end-to-end DAL plugin testing"
```
