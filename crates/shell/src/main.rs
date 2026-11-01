#[cfg(not(all(feature = "sandbox", target_os = "windows")))]
fn main() -> Result<(), &'static str> {
    cef_host::run()
}

#[cfg(all(feature = "sandbox", target_os = "windows"))]
fn main() -> Result<(), &'static str> {
    Err(
        "With the sandbox on, run the bundle from `cargo xtask bundle`, which starts through bootstrap.exe.",
    )
}
