//! The helper process macOS bundles need. Other platforms relaunch the main executable.

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(cef_host::run_helper());
}

#[cfg(not(target_os = "macos"))]
fn main() {}
