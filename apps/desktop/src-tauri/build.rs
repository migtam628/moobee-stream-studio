use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=swift/MooBeeScreenRecording.swift");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR missing"));
        let swift_target = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64-apple-macos13.0",
            Ok("x86_64") => "x86_64-apple-macos13.0",
            other => panic!("Unsupported macOS recorder architecture: {other:?}"),
        };
        let sdk = Command::new("xcrun")
            .args(["--sdk", "macosx", "--show-sdk-path"])
            .output()
            .expect("Xcode command-line SDK not available");
        assert!(sdk.status.success(), "macOS SDK discovery failed");
        let sdk_dir = String::from_utf8(sdk.stdout).expect("Invalid macOS SDK path");
        let sdk_dir = sdk_dir.trim();
        let archive = out_dir.join("libmoobee_sck.a");

        // Compile the FIRST-PARTY Swift recorder directly into the app binary.
        // No helper application, OBS process or FFmpeg executable is installed.
        let result = Command::new("xcrun")
            .args([
                "swiftc", "-swift-version", "5", "-parse-as-library",
                "-target", swift_target, "-sdk", sdk_dir,
                "-O", "-emit-library", "-static",
                "-module-name", "MooBeeSCK",
                "-o",
            ])
            .arg(&archive)
            .arg("swift/MooBeeScreenRecording.swift")
            .status()
            .expect("Swift compiler unavailable");
        assert!(result.success(), "Failed to compile native ScreenCaptureKit recording bridge");

        println!("cargo:rustc-link-search=native={}", out_dir.display());
        println!("cargo:rustc-link-lib=static=moobee_sck");
        // Swift's async runtime uses @rpath/libswift_Concurrency.dylib on
        // macOS. Rust is the final linker here, so it must supply the standard
        // OS Swift runtime search path for both the app and native test binaries.
        // This is a system path, not a developer-machine Xcode dependency.
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    }
    tauri_build::build();
}
