//! Keeps saved passwords in the system's password store: the Keychain on macOS,
//! Credential Manager on Windows and the Secret Service on Linux. Stardust's own
//! database only knows each login's site and username.
//!
//! With `--use-mock-keychain`, as tests run, passwords stay in memory instead, so a
//! test never touches the person's real store.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "Stardust saved passwords";

fn uses_memory() -> bool {
    static MEMORY: OnceLock<bool> = OnceLock::new();
    *MEMORY.get_or_init(|| std::env::args().any(|arg| arg == "--use-mock-keychain"))
}

fn memory() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE
        .get_or_init(Mutex::default)
        .lock()
        .expect("Failed to lock passwords")
}

/// The store's name for a login. A space can't be in an origin, so this can't be
/// confused between sites.
fn account(origin: &str, username: &str) -> String {
    format!("{origin} {username}")
}

pub fn save(origin: &str, username: &str, password: &str) -> Result<(), String> {
    let account = account(origin, username);
    if uses_memory() {
        memory().insert(account, password.to_owned());
        return Ok(());
    }
    keyring::Entry::new(SERVICE, &account)
        .and_then(|entry| entry.set_password(password))
        .map_err(|error| error.to_string())
}

pub fn get(origin: &str, username: &str) -> Option<String> {
    let account = account(origin, username);
    if uses_memory() {
        return memory().get(&account).cloned();
    }
    keyring::Entry::new(SERVICE, &account)
        .and_then(|entry| entry.get_password())
        .ok()
}

pub fn delete(origin: &str, username: &str) {
    let account = account(origin, username);
    if uses_memory() {
        memory().remove(&account);
        return;
    }
    if let Ok(entry) = keyring::Entry::new(SERVICE, &account) {
        let _ = entry.delete_credential();
    }
}
