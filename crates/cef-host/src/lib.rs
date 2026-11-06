//! Starts CEF and wraps its callbacks. Adapted from the `cefsimple` example in
//! `tauri-apps/cef-rs`.

mod app;
mod client;
#[cfg(target_os = "macos")]
mod mac;
mod passwords;
mod render;
mod ui;
mod updates;

use cef::*;

/// Runs the browser until its last window closes. CEF relaunches this executable
/// for its helper processes, so those return from here once their work is done.
pub fn run() -> Result<(), &'static str> {
    // Helper processes start this executable too, with `--type`. Only the browser
    // itself installs updates.
    let is_helper = std::env::args().any(|arg| arg.starts_with("--type="));
    if !is_helper && let Some(executable) = updates::install_downloaded() {
        let restarted = std::process::Command::new(executable)
            .args(std::env::args().skip(1))
            .spawn();
        if restarted.is_ok() {
            return Ok(());
        }
    }

    #[cfg(target_os = "macos")]
    let _library = {
        let exe = std::env::current_exe().map_err(|_| "failed to find executable")?;
        let loader = library_loader::LibraryLoader::new(&exe, false);
        if !loader.load() {
            return Err("failed to load the CEF framework");
        }
        loader
    };

    run_with_args(&args::Args::new(), std::ptr::null_mut())
}

/// Entry point for the Windows sandbox, where CEF's `bootstrap.exe` is the executable.
///
/// # Safety
/// `instance` and `sandbox_info` must be the values `bootstrap.exe` passed to `RunWinMain`.
#[cfg(target_os = "windows")]
pub unsafe fn run_windows(
    instance: sys::HINSTANCE,
    sandbox_info: *mut u8,
) -> Result<(), &'static str> {
    run_with_args(&args::Args::from(MainArgs { instance }), sandbox_info)
}

/// Runs a macOS helper process.
#[cfg(target_os = "macos")]
pub fn run_helper() -> i32 {
    let args = args::Args::new();

    #[cfg(feature = "sandbox")]
    let _sandbox = {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        sandbox
    };

    let exe = std::env::current_exe().expect("failed to find executable");
    let loader = library_loader::LibraryLoader::new(&exe, true);
    assert!(loader.load(), "failed to load the CEF framework");

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = app::BrowserApp::new();
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    )
}

pub(crate) fn data_dir() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("com", "Ambiqa", "Stardust")
        .map(|dirs| dirs.data_dir().to_path_buf())
}

fn run_with_args(args: &args::Args, sandbox_info: *mut u8) -> Result<(), &'static str> {
    // Initialize the CEF API version.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    #[cfg(target_os = "macos")]
    mac::setup_application();

    let cmd_line = args.as_cmd_line().ok_or("failed to parse command line")?;
    let is_browser_process = cmd_line.has_switch(Some(&CefString::from("type"))) != 1;

    let mut app = app::BrowserApp::new();
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), sandbox_info);
    if !is_browser_process {
        return if code >= 0 {
            Ok(())
        } else {
            Err("failed to run helper process")
        };
    }

    let data_dir = data_dir().ok_or("failed to find a data directory")?;
    let root = data_dir.to_string_lossy().into_owned();
    let profile = data_dir.join("Default").to_string_lossy().into_owned();
    let settings = Settings {
        no_sandbox: !cfg!(feature = "sandbox") as _,
        root_cache_path: CefString::from(root.as_str()),
        // Without a profile path, cookies and logins are lost when the browser closes.
        cache_path: CefString::from(profile.as_str()),
        persist_session_cookies: 1,
        ..Default::default()
    };

    let initialized = initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        sandbox_info,
    );
    if initialized != 1 {
        return Err("failed to initialize CEF");
    }

    run_message_loop();
    shutdown();
    updates::restart_if_asked();
    Ok(())
}
