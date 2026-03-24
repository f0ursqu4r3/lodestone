# Lodestone Camera Extension

A macOS Camera Extension (CMIOExtension) that registers "Lodestone Virtual Camera" as a system camera device. Other apps (Zoom, Discord, FaceTime, etc.) can select it as a camera source.

## How It Works

1. The main Lodestone app composites frames and writes them to an **IOSurface** (shared memory)
2. The IOSurface ID is published via **App Group UserDefaults** (`group.com.lodestone.app`)
3. This extension polls the IOSurface at its configured frame rate and delivers frames to consuming apps

## Building

This requires an **Xcode project** — Swift Package Manager cannot produce `.systemextension` bundles.

1. Open Xcode → File → New → Project → macOS → Camera Extension
2. Name it "LodestoneCamera", bundle ID `com.lodestone.camera-extension`
3. Replace the generated source files with the ones in `Sources/`
4. Set the entitlements to `LodestoneCamera.entitlements`
5. Set the Info.plist to the one in this directory
6. Build: `xcodebuild -scheme LodestoneCamera build`

## Development Setup

Camera Extensions are system extensions that require installation:

```bash
# Enable developer mode for system extensions (one-time)
sudo systemextensionsctl developer on

# After building, the .systemextension bundle needs to be:
# 1. Embedded in the main app bundle under Contents/Library/SystemExtensions/
# 2. Or installed via OSSystemExtensionManager.submitRequest()
```

## Entitlements

- `com.apple.developer.cmio.system-extension` — registers as a CoreMediaIO device
- `com.apple.security.application-groups` — shared `group.com.lodestone.app` for IOSurface ID exchange

The `com.apple.developer.cmio.system-extension` entitlement requires a paid Apple Developer account for distribution. Local development works with development signing.

## Testing

```bash
# Verify the camera appears in the system
ffplay -f avfoundation -i "Lodestone Virtual Camera"

# Or open Photo Booth / QuickTime and look for "Lodestone Virtual Camera"
```
