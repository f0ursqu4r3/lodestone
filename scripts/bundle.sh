#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_MODE="release"
SIGN_IDENTITY="Apple Development"

for arg in "$@"; do
    case "$arg" in
        --debug) BUILD_MODE="debug" ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--debug]" >&2
            exit 1
            ;;
    esac
done

XCODE_CONFIG="Release"
if [[ "$BUILD_MODE" == "debug" ]]; then
    XCODE_CONFIG="Debug"
fi

# ─── Paths ────────────────────────────────────────────────────────────────────

RUST_BINARY="${REPO_ROOT}/target/${BUILD_MODE}/lodestone"
EXT_SRC="${REPO_ROOT}/target/xcode-build/Build/Products/${XCODE_CONFIG}/LodestoneCamera.systemextension"
APP_ENTITLEMENTS="${REPO_ROOT}/Lodestone.entitlements"
EXT_ENTITLEMENTS="${REPO_ROOT}/lodestone-camera-extension/LodestoneCamera-signing.entitlements"
APP_BUNDLE="${REPO_ROOT}/Lodestone.app"

# ─── Pre-flight checks ───────────────────────────────────────────────────────

if [[ ! -f "$RUST_BINARY" ]]; then
    echo "error: Rust binary not found at: $RUST_BINARY" >&2
    echo "       Run 'cargo build$([ "$BUILD_MODE" = "release" ] && echo " --release" || echo "")' first." >&2
    exit 1
fi

if [[ ! -d "$EXT_SRC" ]]; then
    echo "error: Camera extension not found at: $EXT_SRC" >&2
    echo "       Run 'cargo build' (build.rs should trigger xcodebuild)." >&2
    exit 1
fi

echo "Building ${BUILD_MODE} app bundle..."

# ─── Clean previous bundle ────────────────────────────────────────────────────

rm -rf "$APP_BUNDLE"

# ─── Create bundle structure ─────────────────────────────────────────────────

mkdir -p \
    "${APP_BUNDLE}/Contents/MacOS" \
    "${APP_BUNDLE}/Contents/Resources" \
    "${APP_BUNDLE}/Contents/Library/SystemExtensions"

# ─── Generate Info.plist ─────────────────────────────────────────────────────

cat > "${APP_BUNDLE}/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.kdougan.lodestone.app</string>
    <key>CFBundleName</key>
    <string>Lodestone</string>
    <key>CFBundleDisplayName</key>
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
    <key>NSSystemExtensionUsageDescription</key>
    <string>Lodestone installs a virtual camera so other apps can use your composited output.</string>
</dict>
</plist>
PLIST

# ─── Copy binary ─────────────────────────────────────────────────────────────

echo "Copying binary..."
cp "$RUST_BINARY" "${APP_BUNDLE}/Contents/MacOS/lodestone"

# ─── Copy system extension ───────────────────────────────────────────────────

echo "Copying system extension..."
cp -R "$EXT_SRC" "${APP_BUNDLE}/Contents/Library/SystemExtensions/"

# ─── Sign — inner bundle first, then outer app ───────────────────────────────

echo "Signing system extension..."
codesign --force --sign "$SIGN_IDENTITY" \
    --entitlements "$EXT_ENTITLEMENTS" \
    --options runtime --timestamp \
    "${APP_BUNDLE}/Contents/Library/SystemExtensions/LodestoneCamera.systemextension"

echo "Signing app bundle..."
codesign --force --deep --sign "$SIGN_IDENTITY" \
    --entitlements "$APP_ENTITLEMENTS" \
    --timestamp \
    "$APP_BUNDLE"

# ─── Verify ──────────────────────────────────────────────────────────────────

echo "Verifying signatures..."
codesign --verify --deep --strict "$APP_BUNDLE"

echo ""
echo "Done: ${APP_BUNDLE}"
echo "Build mode: ${BUILD_MODE}"
