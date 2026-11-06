//! Keeps Stardust up to date: installs a downloaded version at startup, and checks for
//! new ones in the background.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The first check waits until the browser has settled after starting.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// What the updater is doing, as the Settings page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Status {
    /// This build has no update server.
    Off,
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
    },
    /// Installs at the next start.
    Ready {
        version: String,
    },
    Failed {
        message: String,
    },
}

static STATUS: Mutex<Status> = Mutex::new(Status::Idle);
static BUSY: AtomicBool = AtomicBool::new(false);
/// Set when the person asks to restart into a downloaded version.
static RESTART: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    STATUS.lock().expect("Failed to lock update status").clone()
}

fn set_status(status: Status) {
    *STATUS.lock().expect("Failed to lock update status") = status;
}

fn staging() -> Option<PathBuf> {
    crate::data_dir().map(|folder| folder.join("updates"))
}

/// The address of the release list: `--update-url`, or the one built in with
/// `STARDUST_UPDATE_URL`. Releases are signed, so the address doesn't need trusting.
fn manifest_url(command_line_url: Option<String>) -> Option<String> {
    let allowed = |url: &String| update::is_allowed_url(url);
    command_line_url.filter(allowed).or_else(|| {
        option_env!("STARDUST_UPDATE_URL")
            .map(str::to_owned)
            .filter(allowed)
    })
}

/// Installs a version downloaded earlier, before anything else starts. Returns the
/// executable to restart into, if one was installed.
pub fn install_downloaded() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let staging = staging()?;
    // The previous version, kept in case the swap failed, isn't needed once this one
    // has started.
    if let Some(install) = update::install_folder(&executable) {
        let old = install.with_file_name(format!("{}.old", install.file_name()?.to_string_lossy()));
        let _ = std::fs::remove_dir_all(old);
    }
    match update::install_staged(&executable, &staging, VERSION) {
        Ok(new) => new,
        Err(error) => {
            eprintln!("Failed to install the downloaded update: {error}");
            // A broken download would fail every start, so it goes.
            let _ = std::fs::remove_dir_all(&staging);
            None
        }
    }
}

/// Starts checking for updates in the background, if this build has a server.
pub fn start(command_line_url: Option<String>) {
    let Some(url) = manifest_url(command_line_url) else {
        set_status(Status::Off);
        return;
    };
    if let Some((version, _)) =
        staging().and_then(|staging| update::staged_version(&staging, VERSION))
    {
        set_status(Status::Ready { version });
    }
    *URL.lock().expect("Failed to lock update address") = Some(url);
    std::thread::spawn(|| {
        std::thread::sleep(FIRST_CHECK);
        loop {
            check_now();
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

static URL: Mutex<Option<String>> = Mutex::new(None);

/// Checks for a new version and downloads it. Does nothing if a check is running.
pub fn check_now() {
    let Some(url) = URL.lock().expect("Failed to lock update address").clone() else {
        return;
    };
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    if !matches!(status(), Status::Ready { .. }) {
        set_status(Status::Checking);
    }
    let result = (|| {
        let Some(release) = update::check(&url, VERSION)? else {
            return Ok(None);
        };
        let staging =
            staging().ok_or_else(|| update::Error::Io(std::io::Error::other("no data folder")))?;
        if update::staged_version(&staging, VERSION)
            .is_some_and(|(version, _)| version == release.version)
        {
            return Ok(Some(release.version));
        }
        set_status(Status::Downloading {
            version: release.version.clone(),
        });
        update::download(&release, &staging)?;
        Ok::<_, update::Error>(Some(release.version))
    })();
    set_status(match result {
        Ok(Some(version)) => Status::Ready { version },
        Ok(None) => Status::UpToDate,
        Err(error) => Status::Failed {
            message: error.to_string(),
        },
    });
    BUSY.store(false, Ordering::SeqCst);
}

/// Checks without holding up the caller.
pub fn check_in_background() {
    std::thread::spawn(check_now);
}

/// Asks for the browser to start again once it has closed, to install the update.
pub fn restart_after_quit() {
    RESTART.store(true, Ordering::SeqCst);
}

/// Called after the browser has closed.
pub fn restart_if_asked() {
    if !RESTART.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(executable) = std::env::current_exe() {
        let _ = std::process::Command::new(executable).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_use_javascript_names() {
        let json = serde_json::to_string(&Status::Ready {
            version: "0.2.0".into(),
        })
        .unwrap();
        assert_eq!(json, r#"{"state":"ready","version":"0.2.0"}"#);
    }

    #[test]
    fn update_addresses_must_be_safe() {
        assert_eq!(
            manifest_url(Some("http://example.com/manifest.json".into())),
            option_env!("STARDUST_UPDATE_URL").map(str::to_owned)
        );
        assert_eq!(
            manifest_url(Some("http://127.0.0.1:8000/manifest.json".into())).as_deref(),
            Some("http://127.0.0.1:8000/manifest.json")
        );
    }
}
