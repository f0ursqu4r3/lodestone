use std::process::Command;

fn main() {
    // Only run on macOS
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    println!("cargo:rerun-if-changed=lodestone-camera-dal/LodestoneCamera.m");
    println!("cargo:rerun-if-changed=lodestone-camera-dal/Info.plist");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // Build the plugin bundle directory structure.
    let bundle_macos = format!("target/{}/LodestoneCamera.plugin/Contents/MacOS", profile);
    std::fs::create_dir_all(&bundle_macos).expect("failed to create plugin bundle MacOS dir");

    // Copy Info.plist into the bundle.
    let plist_dest = format!("target/{}/LodestoneCamera.plugin/Contents/Info.plist", profile);
    std::fs::copy("lodestone-camera-dal/Info.plist", &plist_dest)
        .expect("failed to copy Info.plist into plugin bundle");

    // Compile the DAL plugin dylib via clang.
    let dylib_dest = format!(
        "target/{}/LodestoneCamera.plugin/Contents/MacOS/LodestoneCamera",
        profile
    );

    let status = Command::new("clang")
        .args([
            "-dynamiclib",
            "-fobjc-arc",
            "-o",
            &dylib_dest,
            "lodestone-camera-dal/LodestoneCamera.m",
            "-framework", "CoreMediaIO",
            "-framework", "CoreMedia",
            "-framework", "CoreVideo",
            "-framework", "IOSurface",
            "-framework", "Foundation",
            "-framework", "CoreFoundation",
        ])
        .status()
        .expect("failed to launch clang");

    if !status.success() {
        panic!("build.rs: clang failed with exit status: {}", status);
    }
}
