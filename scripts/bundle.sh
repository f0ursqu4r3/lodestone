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

# ─── Paths ────────────────────────────────────────────────────────────────────

RUST_BINARY="${REPO_ROOT}/target/${BUILD_MODE}/lodestone"
DAL_PLUGIN_SRC="${REPO_ROOT}/target/${BUILD_MODE}/LodestoneCamera.plugin"
APP_ENTITLEMENTS="${REPO_ROOT}/Lodestone.entitlements"
APP_BUNDLE="${REPO_ROOT}/Lodestone.app"
DAL_INSTALL_DEST="/Library/CoreMediaIO/Plug-Ins/DAL/LodestoneCamera.plugin"

# ─── Pre-flight checks ────────────────────────────────────────────────────────

if [[ ! -f "$RUST_BINARY" ]]; then
    echo "error: Rust binary not found at: $RUST_BINARY" >&2
    echo "       Run 'cargo build$([ "$BUILD_MODE" = "release" ] && echo " --release" || echo "")' first." >&2
    exit 1
fi

if [[ ! -d "$DAL_PLUGIN_SRC" ]]; then
    echo "error: DAL plugin not found at: $DAL_PLUGIN_SRC" >&2
    echo "       Run 'cargo build$([ "$BUILD_MODE" = "release" ] && echo " --release" || echo "")' first." >&2
    exit 1
fi

if [[ ! -f "$APP_ENTITLEMENTS" ]]; then
    echo "error: App entitlements not found at: $APP_ENTITLEMENTS" >&2
    exit 1
fi

echo "Building ${BUILD_MODE} app bundle..."

# ─── Clean previous bundle ────────────────────────────────────────────────────

rm -rf "$APP_BUNDLE"

# ─── Create bundle structure ──────────────────────────────────────────────────

mkdir -p \
    "${APP_BUNDLE}/Contents/MacOS" \
    "${APP_BUNDLE}/Contents/Resources"

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

# ─── Copy binary ──────────────────────────────────────────────────────────────

echo "Copying binary..."
cp "$RUST_BINARY" "${APP_BUNDLE}/Contents/MacOS/lodestone"

# ─── Sign app bundle ──────────────────────────────────────────────────────────

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

# ─── DAL plugin install ───────────────────────────────────────────────────────

echo ""
echo "Checking DAL plugin installation..."

NEEDS_INSTALL=false

if [[ ! -d "$DAL_INSTALL_DEST" ]]; then
    echo "DAL plugin not installed — installing..."
    NEEDS_INSTALL=true
elif ! diff -r "$DAL_INSTALL_DEST" "$DAL_PLUGIN_SRC" > /dev/null 2>&1; then
    echo "DAL plugin differs from installed version — updating..."
    NEEDS_INSTALL=true
else
    echo "DAL plugin is up to date."
fi

if [[ "$NEEDS_INSTALL" == "true" ]]; then
    sudo cp -R "$DAL_PLUGIN_SRC" "/Library/CoreMediaIO/Plug-Ins/DAL/"
    echo "DAL plugin installed to: $DAL_INSTALL_DEST"
    echo ""
    echo "NOTE: Restart any apps that use cameras (e.g. Zoom, Teams, OBS) for the"
    echo "      virtual camera to become available."
fi
