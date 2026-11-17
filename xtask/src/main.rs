//! Project tasks:
//!
//! - `cargo xtask bundle [--release]` builds the browser and copies the CEF runtime next
//!   to it, in `dist/`.
//! - `cargo xtask keygen <private key file>` makes a release signing key, and puts its
//!   public half in `crates/update/release-key.pub`.
//! - `cargo xtask release --key <private key file> --out <folder>` packages `dist/` for
//!   this platform and writes a signed `manifest.json` into the folder, keeping other
//!   platforms' entries for the same version.

use std::path::{Path, PathBuf};

mod release;

const APP: &str = "stardust";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|index| args.get(index + 1))
            .map(PathBuf::from)
    };
    match args.first().map(String::as_str) {
        Some("bundle") => {
            let release = args.iter().any(|arg| arg == "--release");
            let output = Path::new("dist");
            std::fs::create_dir_all(output).expect("failed to create dist");
            println!("Run the browser with {}", bundle(output, release).display());
        }
        Some("keygen") => match args.get(1) {
            Some(path) => release::keygen(Path::new(path)),
            None => usage(),
        },
        Some("release") => match (value("--key"), value("--out")) {
            (Some(key), Some(out)) => release::release(&key, &out),
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() {
    eprintln!(
        "usage:\n  cargo xtask bundle [--release]\n  cargo xtask keygen <private key file>\n  cargo xtask release --key <private key file> --out <folder>"
    );
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
fn bundle(output: &Path, release: bool) -> PathBuf {
    let path =
        cef::build_util::linux::build_bundle(output, APP, release).expect("failed to bundle");
    if release {
        // CEF ships with debug symbols, which are most of its size.
        let status = std::process::Command::new("strip")
            .arg(output.join("libcef.so"))
            .status()
            .expect("failed to run strip");
        assert!(status.success(), "failed to strip libcef.so");
    }
    path
}

#[cfg(target_os = "windows")]
fn bundle(output: &Path, release: bool) -> PathBuf {
    cef::build_util::win::build_bundle(output, APP, release).expect("failed to bundle")
}

// The macOS bundler in cef-rs only builds debug binaries.
#[cfg(target_os = "macos")]
fn bundle(output: &Path, _release: bool) -> PathBuf {
    use cef::build_util::mac::{BundleInfo, build_bundle};

    let version = env!("CARGO_PKG_VERSION").parse().expect("invalid version");
    let info = BundleInfo::new(APP, "com.ambiqa.stardust", "Stardust", "English", version);
    let app = build_bundle(output, APP, info).expect("failed to bundle");
    // macOS shows these when a site first asks to use the camera, microphone or
    // location, and refuses access without them.
    let plist = app.join("Contents/Info.plist");
    for (key, text) in [
        (
            "NSCameraUsageDescription",
            "Websites you allow can use the camera.",
        ),
        (
            "NSMicrophoneUsageDescription",
            "Websites you allow can use the microphone.",
        ),
        (
            "NSLocationUsageDescription",
            "Websites you allow can see your location.",
        ),
        (
            "NSLocationWhenInUseUsageDescription",
            "Websites you allow can see your location.",
        ),
    ] {
        let status = std::process::Command::new("plutil")
            .args(["-replace", key, "-string", text])
            .arg(&plist)
            .status()
            .expect("failed to run plutil");
        assert!(status.success(), "failed to set {key}");
    }
    // The app icon: Finder and the Dock read it from Resources.
    let icon = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/icon/stardust.icns");
    let resources = app.join("Contents/Resources");
    std::fs::create_dir_all(&resources).expect("failed to make Resources");
    std::fs::copy(&icon, resources.join("stardust.icns")).expect("failed to copy the icon");
    let status = std::process::Command::new("plutil")
        .args(["-replace", "CFBundleIconFile", "-string", "stardust"])
        .arg(&plist)
        .status()
        .expect("failed to run plutil");
    assert!(status.success(), "failed to set CFBundleIconFile");
    app
}
