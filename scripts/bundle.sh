#!/usr/bin/env bash
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_MODE="release"
SIGN_IDENTITY="Apple Development"

# ─── Argument parsing ─────────────────────────────────────────────────────────

for arg in "$@"; do
    case "$arg" in
        --debug)
            BUILD_MODE="debug"
            ;;
        *)
            echo "Unknown argument: $arg" >&2
            echo "Usage: $0 [--debug]" >&2
            exit 1
            ;;
    esac
done

XCODE_CONFIG="$(tr '[:lower:]' '[:upper:]' <<< "${BUILD_MODE:0:1}")${BUILD_MODE:1}"  # Capitalize: debug → Debug

# ─── Paths ────────────────────────────────────────────────────────────────────

RUST_BINARY="${REPO_ROOT}/target/${BUILD_MODE}/lodestone"
EXT_SRC="${REPO_ROOT}/target/xcode-build/Build/Products/${XCODE_CONFIG}/LodestoneCamera.appex"
APP_ENTITLEMENTS="${REPO_ROOT}/Lodestone.entitlements"
EXT_ENTITLEMENTS="${REPO_ROOT}/lodestone-camera-extension/LodestoneCamera.entitlements"
APP_BUNDLE="${REPO_ROOT}/Lodestone.app"

# ─── Pre-flight checks ────────────────────────────────────────────────────────

if [[ ! -f "$RUST_BINARY" ]]; then
    echo "error: Rust binary not found at: $RUST_BINARY" >&2
    echo "       Run 'cargo build$([ "$BUILD_MODE" = "release" ] && echo " --release" || echo "")' first." >&2
    exit 1
fi

if [[ ! -d "$EXT_SRC" ]]; then
    echo "error: Camera extension not found at: $EXT_SRC" >&2
    echo "       Run 'cargo build$([ "$BUILD_MODE" = "release" ] && echo " --release" || echo "")' (which builds the Xcode extension) first." >&2
    exit 1
fi

if [[ ! -f "$APP_ENTITLEMENTS" ]]; then
    echo "error: App entitlements not found at: $APP_ENTITLEMENTS" >&2
    exit 1
fi

if [[ ! -f "$EXT_ENTITLEMENTS" ]]; then
    echo "error: Extension entitlements not found at: $EXT_ENTITLEMENTS" >&2
    exit 1
fi

echo "Building ${BUILD_MODE} app bundle..."

# ─── Clean previous bundle ────────────────────────────────────────────────────

rm -rf "$APP_BUNDLE"

# ─── Create bundle structure ──────────────────────────────────────────────────

mkdir -p \
    "${APP_BUNDLE}/Contents/MacOS" \
    "${APP_BUNDLE}/Contents/Resources" \
    "${APP_BUNDLE}/Contents/PlugIns"

# ─── Generate Info.plist ──────────────────────────────────────────────────────

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

# ─── Copy binary and extension ────────────────────────────────────────────────

echo "Copying binary..."
cp "$RUST_BINARY" "${APP_BUNDLE}/Contents/MacOS/lodestone"

echo "Copying camera extension..."
cp -R "$EXT_SRC" "${APP_BUNDLE}/Contents/PlugIns/"

# ─── Sign — inner bundle first, then outer app ────────────────────────────────

EXT_DEST="${APP_BUNDLE}/Contents/PlugIns/LodestoneCamera.appex"

echo "Signing extension bundle..."
codesign \
    --force \
    --sign "$SIGN_IDENTITY" \
    --entitlements "$EXT_ENTITLEMENTS" \
    --options runtime \
    --timestamp \
    "$EXT_DEST"

echo "Signing app bundle..."
codesign \
    --force \
    --sign "$SIGN_IDENTITY" \
    --entitlements "$APP_ENTITLEMENTS" \
    --options runtime \
    --timestamp \
    "$APP_BUNDLE"

# ─── Verify ───────────────────────────────────────────────────────────────────

echo "Verifying signatures..."
codesign --verify --deep --strict "$APP_BUNDLE"

echo ""
echo "Done: ${APP_BUNDLE}"
echo "Build mode: ${BUILD_MODE}"
