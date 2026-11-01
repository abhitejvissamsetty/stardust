//! The DLL that CEF's `bootstrap.exe` loads on Windows. Empty on other platforms.

/// # Safety
/// Called by `bootstrap.exe` with valid arguments.
#[cfg(all(target_os = "windows", feature = "sandbox"))]
#[unsafe(no_mangle)]
unsafe extern "C" fn RunWinMain(
    instance: cef::sys::HINSTANCE,
    _command_line: *const u8,
    _command_show: i32,
    sandbox_info: *mut u8,
) -> i32 {
    match unsafe { cef_host::run_windows(instance, sandbox_info) } {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
