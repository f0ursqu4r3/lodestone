#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGN_ID="6C31FF99174AB0DAEA7A08B034C173BCB6B40AC4"

# Find the host app provisioning profile (com.kdougan.lodestone.app)
HOST_PROFILE=""
for p in ~/Library/Developer/Xcode/UserData/Provisioning\ Profiles/*.provisionprofile; do
    # Match the exact host app identifier (not the extension which contains it as a prefix)
    if security cms -D -i "$p" 2>/dev/null | grep -q "GNM2RZ7D52\.com\.kdougan\.lodestone\.app<"; then
        HOST_PROFILE="$p"
        break
    fi
done

if [[ -z "$HOST_PROFILE" ]]; then
    echo "error: No provisioning profile found for com.kdougan.lodestone.app" >&2
    echo "       Build LodestoneHost in Xcode first to generate it." >&2
    exit 1
fi

echo "Using host profile: $(basename "$HOST_PROFILE")"

# Host app entitlements — system-extension.install for activation
cat > /tmp/host-entitlements.plist <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.system-extension.install</key>
    <true/>
    <key>com.apple.security.cs.disable-library-validation</key>
    <true/>
    <key>com.apple.security.application-groups</key>
    <array>
        <string>group.com.kdougan.lodestone.app</string>
    </array>
</dict>
</plist>
EOF

echo "Removing old app..."
sudo rm -rf /Applications/Lodestone.app

echo "Building app bundle..."
APP="/Applications/Lodestone.app"
sudo mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Library/SystemExtensions"
sudo chown -R "$(whoami)" "$APP"

# Info.plist
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.kdougan.lodestone.app</string>
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
    <string>Lodestone needs camera access.</string>
    <key>NSMicrophoneUsageDescription</key>
    <string>Lodestone needs microphone access.</string>
    <key>NSSystemExtensionUsageDescription</key>
    <string>Lodestone installs a virtual camera.</string>
</dict>
</plist>
PLIST

# Copy binary
cp "$REPO_ROOT/target/debug/lodestone" "$APP/Contents/MacOS/"

# Copy system extension (already has embedded.provisionprofile from xcodebuild)
cp -R "$REPO_ROOT/target/xcode-build/Build/Products/Debug/LodestoneCamera.systemextension" \
    "$APP/Contents/Library/SystemExtensions/"

# Embed the host app provisioning profile
cp "$HOST_PROFILE" "$APP/Contents/embedded.provisionprofile"

# DO NOT re-sign the extension — use exactly what Xcode built (with its embedded profile)

# Sign host app with system-extension.install + provisioning profile
echo "Signing app (without hardened runtime to allow restricted entitlements)..."
codesign --force --sign "$SIGN_ID" \
    --entitlements /tmp/host-entitlements.plist \
    "$APP"

echo "Verifying..."
codesign --verify --deep --strict "$APP"

echo ""
echo "Launching..."
open "$APP"
