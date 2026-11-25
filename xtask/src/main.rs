//! Project tasks:
//!
//! - `cargo xtask bundle [--release]` builds the browser and copies the CEF runtime next
//!   to it, in `dist/`.
//! - `cargo xtask keygen <private key file>` makes a release signing key, and puts its
//!   public half in `crates/update/release-key.pub`.
//! - `cargo xtask package --out <folder>` packages `dist/` for this platform, with a
//!   `.package.json` describing it. It needs no key, so build servers run it.
//! - `cargo xtask sign --key <private key file> --dir <folder> [--rollout <percent>]`
//!   gathers the folder's `.package.json` files into `manifest.json` and signs it. See
//!   `docs/releasing.md`.

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
        Some("package") => match value("--out") {
            Some(out) => release::package_release(&out),
            None => usage(),
        },
        Some("sign") => {
            let rollout = match value("--rollout") {
                Some(text) => text.to_string_lossy().parse().unwrap_or(0),
                None => 100,
            };
            match (value("--key"), value("--dir")) {
                (Some(key), Some(dir)) => release::sign(&key, &dir, rollout),
                _ => usage(),
            }
        }
        _ => usage(),
    }
}

fn usage() {
    eprintln!(
        "usage:\n  cargo xtask bundle [--release]\n  cargo xtask keygen <private key file>\n  cargo xtask package --out <folder>\n  cargo xtask sign --key <private key file> --dir <folder> [--rollout <percent>]"
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

// cef-rs's `build_bundle` builds a package named after the app, but Stardust's
// is `shell`, so this builds it and then uses cef-rs only to lay out the files.
#[cfg(target_os = "windows")]
fn bundle(output: &Path, release: bool) -> PathBuf {
    let mut args = vec!["build", "-p", "shell", "--lib"];
    if release {
        args.push("--release");
    }
    let status = std::process::Command::new(env!("CARGO"))
        .args(&args)
        .status()
        .expect("failed to run cargo");
    assert!(status.success(), "failed to build Stardust");
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target")
        .join(if release { "release" } else { "debug" });
    cef::build_util::win::bundle(output, &target, APP).expect("failed to bundle")
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
        // The name in the menu bar and the Dock.
        ("CFBundleName", "Stardust"),
        ("CFBundleDisplayName", "Stardust"),
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
