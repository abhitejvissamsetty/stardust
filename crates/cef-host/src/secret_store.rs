//! Checks, on Linux, that the system's password store answers before Chromium is
//! given it. Chromium keeps its cookie key there and saved passwords live there too.
//! When the store is locked and its unlock prompt can't be shown, or there's no
//! keyring to unlock, Chromium waits for it forever and no page finishes loading.
//! So Stardust asks first, with a time limit, and falls back to Chromium's built-in
//! key when the store is locked or doesn't answer.

use std::sync::OnceLock;

/// How long the store has to answer before Stardust gives up on it.
#[cfg(target_os = "linux")]
const ANSWER_SECONDS: u64 = 3;

static AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Whether the system's password store can be used. It's checked once, when first asked.
pub fn available() -> bool {
    *AVAILABLE.get_or_init(check)
}

#[cfg(not(target_os = "linux"))]
fn check() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn check() -> bool {
    use dbus_secret_service::{EncryptionType, SecretService};
    use std::sync::mpsc;
    use std::time::Duration;

    let (sender, receiver) = mpsc::channel();
    // A store that never answers would hold up this thread, so it isn't waited for.
    std::thread::spawn(move || {
        let usable = (|| {
            // A prompt to unlock the store can fail to appear and then never return, so
            // a locked store isn't unlocked here. The person is told instead.
            let service =
                SecretService::connect_with_max_prompt_timeout(EncryptionType::Plain, 0).ok()?;
            // Without a default keyring, Chromium would ask to create one, which can wait forever too.
            let collection = service.get_default_collection().ok()?;
            Some(!collection.is_locked().ok()?)
        })();
        let _ = sender.send(usable == Some(true));
    });
    receiver
        .recv_timeout(Duration::from_secs(ANSWER_SECONDS))
        .unwrap_or(false)
}

/// Whether Chromium would use KWallet, which this doesn't check, rather than the Secret Service.
pub fn uses_kwallet(desktop: Option<&str>) -> bool {
    desktop.is_some_and(|names| {
        names
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_kde_among_desktops() {
        assert!(uses_kwallet(Some("KDE")));
        assert!(uses_kwallet(Some("ubuntu:kde")));
        assert!(!uses_kwallet(Some("GNOME")));
        assert!(!uses_kwallet(Some("")));
        assert!(!uses_kwallet(None));
    }
}
