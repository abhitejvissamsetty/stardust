//! Builds the browser and copies the CEF runtime next to it: `cargo xtask bundle [--release]`.

use std::path::{Path, PathBuf};

const APP: &str = "stardust";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("bundle") {
        eprintln!("usage: cargo xtask bundle [--release]");
        std::process::exit(2);
    }
    let release = args.iter().any(|arg| arg == "--release");

    let output = Path::new("dist");
    std::fs::create_dir_all(output).expect("failed to create dist");
    println!("Run the browser with {}", bundle(output, release).display());
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
    build_bundle(output, APP, info).expect("failed to bundle")
}
