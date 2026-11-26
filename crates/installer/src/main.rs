//! Stardust's graphical installer for Linux. `cargo xtask installer` appends the
//! browser to this program, making one `.run` file to download and open.

#[cfg(target_os = "linux")]
mod app;
#[cfg(target_os = "linux")]
mod install;
#[cfg(target_os = "linux")]
mod payload;

#[cfg(target_os = "linux")]
fn main() -> eframe::Result {
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This installer is for Linux.");
    std::process::exit(1);
}
