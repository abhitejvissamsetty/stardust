//! Installs Stardust for the person running the installer: the browser in a folder
//! they own, so it can update itself, plus a menu entry, a `stardust` command and an
//! uninstaller. Nothing needs an administrator except the optional AppArmor rule.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::payload::Payload;

pub const AGREEMENT: &str = include_str!("../../../legal/EULA.txt");
/// The agreement's version, recorded when someone accepts it.
pub const AGREEMENT_VERSION: &str = "1.0";
const ICON_128: &[u8] = include_bytes!("../../../assets/icon/stardust-128.png");
const ICON_256: &[u8] = include_bytes!("../../../assets/icon/stardust-256.png");

pub struct Options {
    pub dir: PathBuf,
    pub menu: bool,
    pub command: bool,
    pub apparmor: bool,
}

#[derive(Default)]
pub struct Progress {
    pub fraction: f32,
    pub status: String,
    /// Set once the install has finished: `Ok` with any warnings, or what went wrong.
    pub done: Option<Result<Vec<String>, String>>,
}

pub type SharedProgress = Arc<Mutex<Progress>>;

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// Where Stardust goes unless the person picks another folder.
pub fn default_dir() -> PathBuf {
    home().join(".local/opt/stardust")
}

/// Whether this system stops Chromium's sandbox from starting without an AppArmor
/// rule, as Ubuntu does from 23.10.
pub fn needs_apparmor() -> bool {
    fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
        .is_ok_and(|value| value.trim() == "1")
}

/// Whether a Stardust from `dir` is running, which would stop it being replaced.
pub fn is_running(dir: &Path) -> bool {
    let program = dir.join("stardust");
    fs::read_dir("/proc").is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| fs::read_link(entry.path().join("exe")).is_ok_and(|exe| exe == program))
    })
}

/// Installs in the background, reporting to `progress`.
pub fn start(options: Options, payload: Payload, progress: SharedProgress) {
    std::thread::spawn(move || {
        let result = install(&options, &payload, &progress);
        let mut progress = progress.lock().expect("Failed to lock progress");
        progress.fraction = 1.0;
        progress.done = Some(result);
    });
}

fn report(progress: &SharedProgress, fraction: f32, status: &str) {
    let mut progress = progress.lock().expect("Failed to lock progress");
    progress.fraction = fraction;
    progress.status = status.to_owned();
}

fn install(
    options: &Options,
    payload: &Payload,
    progress: &SharedProgress,
) -> Result<Vec<String>, String> {
    let dir = &options.dir;
    if is_running(dir) {
        return Err("Stardust is open. Close it, then try again.".into());
    }
    let parent = dir
        .parent()
        .ok_or("Choose a folder inside another folder.")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Couldn't create {}: {error}", parent.display()))?;
    let name = dir
        .file_name()
        .ok_or("Choose a folder to install into.")?
        .to_string_lossy();
    let staging = parent.join(format!(".{name}-installing"));
    let old = parent.join(format!(".{name}-old"));
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&old);

    report(progress, 0.0, "Unpacking Stardust…");
    unpack(payload, &staging, progress).map_err(|error| {
        let _ = fs::remove_dir_all(&staging);
        format!("Couldn't unpack Stardust: {error}")
    })?;

    report(progress, 0.92, "Recording the licence agreement…");
    let write = |name: &str, contents: &[u8]| {
        fs::write(staging.join(name), contents)
            .map_err(|error| format!("Couldn't write {name}: {error}"))
    };
    write("LICENSE-AGREEMENT.txt", AGREEMENT.as_bytes())?;
    let accepted = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    write(
        ".agreement-accepted",
        format!("agreement={AGREEMENT_VERSION}\naccepted={accepted}\n").as_bytes(),
    )?;
    write("uninstall", uninstaller(dir).as_bytes())?;
    fs::set_permissions(staging.join("uninstall"), fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("Couldn't write the uninstaller: {error}"))?;

    // The new version replaces any old one in one step, and the old one goes after.
    report(progress, 0.95, "Putting Stardust in place…");
    if dir.exists() {
        fs::rename(dir, &old)
            .map_err(|error| format!("Couldn't replace {}: {error}", dir.display()))?;
    }
    if let Err(error) = fs::rename(&staging, dir) {
        let _ = fs::rename(&old, dir);
        return Err(format!("Couldn't install into {}: {error}", dir.display()));
    }
    let _ = fs::remove_dir_all(&old);

    let mut warnings = Vec::new();
    if options.menu {
        report(progress, 0.97, "Adding Stardust to the app menu…");
        if let Err(error) = add_to_menu(dir) {
            warnings.push(format!("Stardust isn't in the app menu: {error}"));
        }
    }
    if options.command
        && let Err(error) = add_command(dir)
    {
        warnings.push(error);
    }
    if options.apparmor {
        report(progress, 0.99, "Asking to let Stardust's sandbox start…");
        if let Err(error) = add_apparmor_rule(dir) {
            warnings.push(format!(
                "Stardust may not start until its AppArmor rule is added: {error}"
            ));
        }
    }
    report(progress, 1.0, "Done");
    Ok(warnings)
}

/// Counts bytes read, so unpacking can show how far it's got.
struct Counting<R> {
    inner: R,
    read: u64,
    total: u64,
    progress: SharedProgress,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        self.read += count as u64;
        let fraction = self.read as f32 / self.total.max(1) as f32;
        if let Ok(mut progress) = self.progress.lock() {
            progress.fraction = fraction * 0.9;
        }
        Ok(count)
    }
}

/// Unpacks the package's one top-level folder into `staging`.
fn unpack(payload: &Payload, staging: &Path, progress: &SharedProgress) -> io::Result<()> {
    fs::create_dir_all(staging)?;
    let reader = Counting {
        inner: payload.open()?,
        read: 0,
        total: payload.length,
        progress: progress.clone(),
    };
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(reader));
    archive.set_preserve_permissions(true);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        // Leave out the top-level folder, and refuse anything that would land
        // outside the staging folder.
        let mut parts = path.components();
        parts.next();
        let relative: PathBuf = parts.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(io::Error::other(format!("unsafe path {}", path.display())));
        }
        let target = staging.join(&relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&target)?;
    }
    Ok(())
}

fn add_to_menu(dir: &Path) -> io::Result<()> {
    let share = home().join(".local/share");
    for (size, png) in [("128x128", ICON_128), ("256x256", ICON_256)] {
        let folder = share.join("icons/hicolor").join(size).join("apps");
        fs::create_dir_all(&folder)?;
        fs::write(folder.join("stardust.png"), png)?;
    }
    let applications = share.join("applications");
    fs::create_dir_all(&applications)?;
    let program = dir.join("stardust");
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Stardust\n\
         GenericName=Web Browser\n\
         Comment=A browser for people who make things\n\
         Exec=\"{}\" %U\n\
         Icon=stardust\n\
         Terminal=false\n\
         Categories=Network;WebBrowser;\n\
         MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;\n\
         StartupWMClass=Stardust\n\
         StartupNotify=true\n",
        program.display()
    );
    fs::write(applications.join("stardust.desktop"), entry)?;
    // Menus pick the entry and icon up sooner with these, where the system has them.
    let quietly = |program: &str, args: &[&Path]| {
        let _ = Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    };
    quietly("update-desktop-database", &[&applications]);
    quietly("gtk-update-icon-cache", &[&share.join("icons/hicolor")]);
    Ok(())
}

fn add_command(dir: &Path) -> Result<(), String> {
    let bin = home().join(".local/bin");
    let link = bin.join("stardust");
    fs::create_dir_all(&bin)
        .map_err(|error| format!("The stardust command wasn't added: {error}"))?;
    match fs::symlink_metadata(&link) {
        Ok(meta) if !meta.file_type().is_symlink() => {
            return Err(format!(
                "The stardust command wasn't added: {} already exists.",
                link.display()
            ));
        }
        Ok(_) => fs::remove_file(&link)
            .map_err(|error| format!("The stardust command wasn't added: {error}"))?,
        Err(_) => {}
    }
    std::os::unix::fs::symlink(dir.join("stardust"), &link)
        .map_err(|error| format!("The stardust command wasn't added: {error}"))
}

/// The AppArmor rule that lets Stardust's sandbox create user namespaces, as Chrome's
/// own package adds for Chrome.
pub fn apparmor_rule(dir: &Path) -> String {
    format!(
        "# Lets Stardust's Chromium sandbox start on systems that restrict user namespaces.\n\
         # Added by the Stardust installer, and removed by its uninstaller.\n\
         abi <abi/4.0>,\n\
         include <tunables/global>\n\
         \n\
         profile stardust \"{}\" flags=(unconfined) {{\n\
         \x20 userns,\n\
         \n\
         \x20 include if exists <local/stardust>\n\
         }}\n",
        dir.join("stardust").display()
    )
}

/// Adds the AppArmor rule as administrator, asking for a password through `pkexec`.
fn add_apparmor_rule(dir: &Path) -> Result<(), String> {
    let mut child = Command::new("pkexec")
        .args([
            "sh",
            "-c",
            "cat > /etc/apparmor.d/stardust && apparmor_parser -r /etc/apparmor.d/stardust",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("couldn't ask for permission ({error})"))?;
    child
        .stdin
        .take()
        .ok_or("couldn't send the rule")?
        .write_all(apparmor_rule(dir).as_bytes())
        .map_err(|error| error.to_string())?;
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let reason = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if reason.is_empty() {
            "permission wasn't given".into()
        } else {
            reason
        })
    }
}

/// Puts `text` in single quotes for the shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// A script that removes what the installer added. People's data stays.
pub fn uninstaller(dir: &Path) -> String {
    let dir = quoted(&dir.to_string_lossy());
    format!(
        r#"#!/bin/sh
# Removes Stardust. Your bookmarks, history and other data stay in
# ~/.local/share/stardust: delete that folder too to remove them.
set -e
dir={dir}
share="${{XDG_DATA_HOME:-$HOME/.local/share}}"
rm -f "$share/applications/stardust.desktop"
rm -f "$share/icons/hicolor/128x128/apps/stardust.png" "$share/icons/hicolor/256x256/apps/stardust.png"
if [ "$(readlink "$HOME/.local/bin/stardust" 2>/dev/null)" = "$dir/stardust" ]; then
  rm -f "$HOME/.local/bin/stardust"
fi
if [ -f /etc/apparmor.d/stardust ]; then
  echo "Removing Stardust's AppArmor rule needs your password."
  pkexec sh -c 'apparmor_parser -R /etc/apparmor.d/stardust 2>/dev/null; rm -f /etc/apparmor.d/stardust' ||
    echo "The rule in /etc/apparmor.d/stardust is still there."
fi
rm -rf "$dir"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$share/applications" || true
echo "Stardust is uninstalled. Your data is still in ~/.local/share/stardust."
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("stardust-installer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn package(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    /// A program with a package appended, as `cargo xtask installer` makes it.
    fn installer_with(dir: &Path, package: &[u8]) -> Payload {
        let program = b"pretend program";
        let mut file = program.to_vec();
        file.extend(package);
        file.extend(payload::trailer(program.len() as u64, package.len() as u64));
        let path = dir.join("installer.run");
        fs::write(&path, file).unwrap();
        Payload {
            file: path,
            offset: program.len() as u64,
            length: package.len() as u64,
        }
    }

    #[test]
    fn installs_the_package_and_records_the_agreement() {
        let dir = temp("install");
        let payload = installer_with(
            &dir,
            &package(&[
                ("dist/stardust", b"browser"),
                ("dist/locales/en.pak", b"en"),
            ]),
        );
        let target = dir.join("opt/stardust");
        let options = Options {
            dir: target.clone(),
            menu: false,
            command: false,
            apparmor: false,
        };
        let progress = SharedProgress::default();
        assert_eq!(install(&options, &payload, &progress), Ok(vec![]));
        assert_eq!(fs::read(target.join("stardust")).unwrap(), b"browser");
        assert_eq!(fs::read(target.join("locales/en.pak")).unwrap(), b"en");
        assert!(
            fs::read_to_string(target.join(".agreement-accepted"))
                .unwrap()
                .contains("agreement=1.0")
        );
        assert!(
            fs::read_to_string(target.join("LICENSE-AGREEMENT.txt"))
                .unwrap()
                .contains("Ambiqa Innovations")
        );
        assert!(
            fs::metadata(target.join("uninstall"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111
                != 0
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replaces_an_older_install() {
        let dir = temp("replace");
        let target = dir.join("stardust");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("leftover"), b"old").unwrap();
        let payload = installer_with(&dir, &package(&[("dist/stardust", b"new")]));
        let options = Options {
            dir: target.clone(),
            menu: false,
            command: false,
            apparmor: false,
        };
        install(&options, &payload, &SharedProgress::default()).unwrap();
        assert_eq!(fs::read(target.join("stardust")).unwrap(), b"new");
        assert!(!target.join("leftover").exists());
        assert!(!dir.join(".stardust-old").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_paths_out_of_the_package() {
        let dir = temp("unsafe");
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        // Written raw, since tar's own checks would refuse this path.
        header.as_gnu_mut().unwrap().name[..17].copy_from_slice(b"dist/../../escape");
        header.set_cksum();
        builder.append(&header, &b"x"[..]).unwrap();
        let bytes = builder.into_inner().unwrap().finish().unwrap();
        let payload = installer_with(&dir, &bytes);
        let options = Options {
            dir: dir.join("stardust"),
            menu: false,
            command: false,
            apparmor: false,
        };
        assert!(install(&options, &payload, &SharedProgress::default()).is_err());
        assert!(!dir.parent().unwrap().join("escape").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn quotes_install_folders_for_the_uninstaller() {
        let script = uninstaller(Path::new("/home/a b/it's/stardust"));
        assert!(script.contains(r#"dir='/home/a b/it'\''s/stardust'"#));
        assert!(
            apparmor_rule(Path::new("/opt/stardust"))
                .contains("profile stardust \"/opt/stardust/stardust\"")
        );
    }
}
