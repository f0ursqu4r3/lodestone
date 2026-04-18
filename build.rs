use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // On Windows, compile the game capture hook DLL directly with rustc.
    // Avoid spawning a nested Cargo build from inside Cargo, which can deadlock
    // on the workspace artifact lock and leave `lodestone(build)` hanging.
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
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
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
        .expect("failed to run xcodebuild - is Xcode installed?");

    if !status.success() {
        panic!("xcodebuild failed with status: {}", status);
    }
}

/// Build the lodestone-hook DLL (game capture hook) on Windows.
fn build_hook_dll() {
    println!("cargo:rerun-if-changed=lodestone-hook/Cargo.toml");
    println!("cargo:rerun-if-changed=lodestone-hook/src/lib.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/d3d11.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/win32.rs");
    println!("cargo:rerun-if-changed=lodestone-hook/src/shared.rs");
    println!("cargo:rerun-if-env-changed=LODESTONE_SKIP_HOOK_BUILD");
    println!("cargo:rerun-if-env-changed=RUSTC");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=OPT_LEVEL");
    println!("cargo:rerun-if-env-changed=DEBUG");
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");

    if env::var_os("LODESTONE_SKIP_HOOK_BUILD").is_some() {
        println!(
            "cargo:warning=Skipping lodestone-hook build because LODESTONE_SKIP_HOOK_BUILD is set"
        );
        return;
    }

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let Some(profile_dir) = profile_output_dir(&out_dir) else {
        println!(
            "cargo:warning=Could not determine Cargo profile output directory from OUT_DIR={}",
            out_dir.display()
        );
        return;
    };

    if let Err(e) = fs::create_dir_all(profile_dir) {
        println!(
            "cargo:warning=Failed to prepare hook DLL output directory {}: {e}",
            profile_dir.display()
        );
        return;
    }

    let rustc = env::var_os("RUSTC").unwrap_or_else(|| OsString::from("rustc"));
    let target = env::var("TARGET").expect("TARGET not set");
    let opt_level = env::var("OPT_LEVEL").unwrap_or_else(|_| "0".to_string());
    let debug = env::var("DEBUG").unwrap_or_default();
    let hook_src = manifest_dir.join("lodestone-hook").join("src").join("lib.rs");

    let mut cmd = Command::new(rustc);
    cmd.arg("--crate-name")
        .arg("lodestone_hook")
        .arg("--crate-type")
        .arg("cdylib")
        .arg("--edition")
        .arg("2024")
        .arg("--target")
        .arg(&target)
        .arg("--out-dir")
        .arg(profile_dir)
        .arg("-C")
        .arg(format!("opt-level={opt_level}"));

    if debug != "0" && !debug.eq_ignore_ascii_case("false") {
        cmd.arg("-C").arg("debuginfo=2");
    }

    append_encoded_rustflags(&mut cmd);
    cmd.arg(&hook_src);

    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) => {
            println!(
                "cargo:warning=Failed to run rustc for lodestone-hook - game capture will not work: {e}"
            );
            return;
        }
    };

    if !status.success() {
        println!(
            "cargo:warning=lodestone-hook build failed with status {status} - game capture will not work"
        );
    }
}

fn profile_output_dir(out_dir: &Path) -> Option<&Path> {
    out_dir.ancestors().nth(3)
}

fn append_encoded_rustflags(cmd: &mut Command) {
    let Some(encoded) = env::var_os("CARGO_ENCODED_RUSTFLAGS") else {
        return;
    };

    for flag in encoded.to_string_lossy().split('\u{1f}') {
        if !flag.is_empty() {
            cmd.arg(flag);
        }
    }
}
