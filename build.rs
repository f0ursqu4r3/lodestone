use std::process::Command;

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // On Windows, build the game capture hook DLL.
    if target_os == "windows" {
        build_hook_dll();
    }

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
    println!(
        "cargo:rerun-if-changed=lodestone-camera-extension/LodestoneCamera.xcodeproj/project.pbxproj"
    );

    // Link SystemExtensions framework for activation code
    println!("cargo:rustc-link-lib=framework=SystemExtensions");

    // Map Cargo profile to Xcode configuration
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let xcode_config = if profile == "release" {
        "Release"
    } else {
        "Debug"
    };

    let status = Command::new("xcodebuild")
        .args([
            "-project",
            "lodestone-camera-extension/LodestoneCamera.xcodeproj",
            "-scheme",
            "LodestoneCamera",
            "-configuration",
            xcode_config,
            "-derivedDataPath",
            "target/xcode-build",
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

/// Build the lodestone-hook DLL (game capture hook) on Windows.
fn build_hook_dll() {
    println!("cargo:rerun-if-changed=lodestone-hook/src/lib.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/d3d11.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/win32.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/shared.rs");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let mut args = vec!["build", "-p", "lodestone-hook"];
    if profile == "release" {
        args.push("--release");
    }

    let status = Command::new("cargo")
        .args(&args)
        .status()
        .expect("failed to build lodestone-hook — is Cargo available?");

    if !status.success() {
        eprintln!("cargo:warning=lodestone-hook build failed — game capture will not work");
    }
}
