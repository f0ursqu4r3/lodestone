use std::process::Command;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    // Re-run when Swift sources or project config change
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/main.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Provider.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Device.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Sources/Stream.swift");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/Info.plist");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.entitlements");
    println!("cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.xcodeproj/project.pbxproj");

    // Link SystemExtensions framework for activation code
    println!("cargo:rustc-link-lib=framework=SystemExtensions");

    // Map Cargo profile to Xcode configuration
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let xcode_config = if profile == "release" { "Release" } else { "Debug" };

    let status = Command::new("xcodebuild")
        .args([
            "-project", "lodestone-camera-extension/LodestoneCamera.xcodeproj",
            "-scheme", "LodestoneCamera",
            "-configuration", xcode_config,
            "-derivedDataPath", "target/xcode-build",
            "-allowProvisioningUpdates",
            "-quiet",
            "build",
        ])
        .status()
        .expect("failed to run xcodebuild — is Xcode installed?");

    if !status.success() {
        panic!("xcodebuild failed with status: {}", status);
    }
}
