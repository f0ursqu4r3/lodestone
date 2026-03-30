#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGN_ID="6C31FF99174AB0DAEA7A08B034C173BCB6B40AC4"
APP="/Applications/Lodestone.app"
XCODE_HOST="$REPO_ROOT/target/xcode-build/Build/Products/Debug/LodestoneHost.app"
XCODE_EXT="$REPO_ROOT/target/xcode-build/Build/Products/Debug/LodestoneCamera.systemextension"

# Clean
sudo rm -rf "$APP"

# Start from Xcode-built host app (has correct provisioning profile + signing)
cp -R "$XCODE_HOST" "$APP"

# Swap in our Rust binary (named to match CFBundleExecutable)
cp "$REPO_ROOT/target/debug/lodestone" "$APP/Contents/MacOS/LodestoneHost"

# Embed the system extension
mkdir -p "$APP/Contents/Library/SystemExtensions"
cp -R "$XCODE_EXT" "$APP/Contents/Library/SystemExtensions/"

# Add plist keys Xcode doesn't include
/usr/libexec/PlistBuddy -c "Add :NSCameraUsageDescription string 'Camera access needed'" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Mic access needed'" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSSystemExtensionUsageDescription string 'Virtual camera'" "$APP/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSHighResolutionCapable bool true" "$APP/Contents/Info.plist"

# Re-sign with entitlements matching what Xcode would use + disable-library-validation for GStreamer
cat > /tmp/full-entitlements.plist <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.application-identifier</key>
    <string>GNM2RZ7D52.com.kdougan.lodestone.app</string>
    <key>com.apple.developer.system-extension.install</key>
    <true/>
    <key>com.apple.developer.team-identifier</key>
    <string>GNM2RZ7D52</string>
    <key>com.apple.security.application-groups</key>
    <array>
        <string>group.com.kdougan.lodestone.app</string>
    </array>
    <key>com.apple.security.cs.disable-library-validation</key>
    <true/>
    <key>com.apple.security.get-task-allow</key>
    <true/>
</dict>
</plist>
EOF

echo "Signing..."
codesign --force --sign "$SIGN_ID" \
    --entitlements /tmp/full-entitlements.plist \
    "$APP"

echo "Verifying..."
codesign --verify --deep --strict "$APP"

echo "Launching..."
open "$APP"
