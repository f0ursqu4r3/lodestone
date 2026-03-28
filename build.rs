use std::process::Command;

fn main() {
    // Only run on macOS
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    // Declare rerun-if-changed for Swift sources and project files
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/main.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Provider.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Device.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Stream.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Info.plist");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.entitlements");
    println!(
        "cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.xcodeproj/project.pbxproj"
    );

    // Verify we have a full Xcode.app, not just CommandLineTools
    let xcode_select = Command::new("xcode-select")
        .arg("-p")
        .output()
        .expect("failed to run xcode-select");

    let xcode_path = String::from_utf8_lossy(&xcode_select.stdout);
    let xcode_path = xcode_path.trim();

    if !xcode_path.contains("Xcode.app") {
        panic!(
            "build.rs: xcode-select points to '{}', which does not appear to be a full \
             Xcode.app installation. The camera extension requires Xcode (not just \
             CommandLineTools) to build. Run: sudo xcode-select -s /Applications/Xcode.app",
            xcode_path
        );
    }

    // Map Cargo profile to Xcode configuration
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let configuration = if profile == "release" { "Release" } else { "Debug" };

    // Run xcodebuild
    let status = Command::new("xcodebuild")
        .args([
            "-project",
            "lodestone-camera-extension/LodestoneCamera.xcodeproj",
            "-scheme",
            "LodestoneCamera",
            "-configuration",
            configuration,
            "-derivedDataPath",
            "target/xcode-build",
            "-allowProvisioningUpdates",
            "-quiet",
            "build",
        ])
        .status()
        .expect("failed to launch xcodebuild");

    if !status.success() {
        panic!(
            "build.rs: xcodebuild failed with exit status: {}",
            status
        );
    }
}
