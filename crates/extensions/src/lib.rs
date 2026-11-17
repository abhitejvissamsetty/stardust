//! Chrome extensions for Stardust: reading their manifests, unpacking `.zip` and `.crx`
//! packages safely, working out the ID Chromium gives them, and keeping the list of
//! installed ones.
//!
//! Extensions are loaded by Chromium itself, which Stardust asks to load each enabled
//! extension's folder when it starts. This crate never runs extension code. It only
//! reads files, so everything an untrusted package can do here is bounded: how many
//! files, how large, and where they may be written.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Largest package, and largest total size once unpacked.
pub const MAX_PACKAGE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 32;
const MAX_ICON_BYTES: u64 = 256 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    /// The package isn't a usable extension. The message is for the person.
    Invalid(String),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "{message}"),
            Self::Io(error) => write!(f, "couldn't read or write files: {error}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn invalid<T>(message: &str) -> Result<T> {
    Err(Error::Invalid(message.to_owned()))
}

/// What an extension's manifest says about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub name: String,
    pub version: String,
    pub description: String,
    pub manifest_version: u32,
    pub permissions: Vec<String>,
    /// Sites the extension can read and change: its host permissions and the sites its
    /// content scripts run on.
    pub host_patterns: Vec<String>,
    /// The page shown when the extension's button is clicked, relative to its folder.
    pub popup: Option<String>,
    pub icon: Option<String>,
    pub action_title: Option<String>,
    /// The `key` that fixes the extension's ID, if it has one.
    #[serde(skip)]
    pub key: Option<String>,
    pub warnings: Vec<String>,
    /// Whether it can read and change every website.
    pub all_sites: bool,
}

// -- Manifests ---------------------------------------------------------------------

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

/// Replaces `__MSG_name__` with the extension's own translation, if it has one.
fn localized(
    value: String,
    default_locale: Option<&str>,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> String {
    let Some(key) = value
        .strip_prefix("__MSG_")
        .and_then(|rest| rest.strip_suffix("__"))
    else {
        return value;
    };
    let Some(locale) = default_locale else {
        return value;
    };
    let messages = read(&format!("_locales/{locale}/messages.json"))
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    // Message names aren't case sensitive.
    messages
        .and_then(|messages| {
            messages.as_object()?.iter().find_map(|(name, entry)| {
                name.eq_ignore_ascii_case(key)
                    .then(|| entry.get("message")?.as_str().map(str::to_owned))?
            })
        })
        .unwrap_or(value)
}

/// A path inside an extension's folder that can be read or shown: relative, with no way
/// out of the folder.
pub fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains("://")
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn largest_icon(icons: &serde_json::Value) -> Option<String> {
    let mut sized: Vec<(u32, String)> = match icons {
        serde_json::Value::String(path) => vec![(0, path.clone())],
        serde_json::Value::Object(map) => map
            .iter()
            .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?.to_owned())))
            .collect(),
        _ => return None,
    };
    sized.retain(|(_, path)| is_safe_relative_path(path));
    sized.sort();
    // Big enough to look sharp, without carrying a huge file around.
    let best = sized
        .iter()
        .rev()
        .find(|(size, _)| *size <= 128)
        .or_else(|| sized.first())?;
    Some(best.1.clone())
}

/// Reads a manifest. `read` gives other files in the extension, by relative path, for
/// translations.
pub fn parse_manifest(manifest: &[u8], read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Summary> {
    if manifest.len() as u64 > MAX_MANIFEST_BYTES {
        return invalid("The manifest is too large.");
    }
    // Some extensions begin with a byte order mark, which JSON doesn't allow.
    let manifest = manifest.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(manifest);
    let value: serde_json::Value = serde_json::from_slice(manifest)
        .map_err(|_| Error::Invalid("manifest.json isn't valid JSON.".into()))?;
    let manifest_version = value
        .get("manifest_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    if manifest_version != 2 && manifest_version != 3 {
        return invalid("This isn't a Chrome extension: its manifest_version must be 2 or 3.");
    }
    let (Some(name), Some(version)) = (text(&value, "name"), text(&value, "version")) else {
        return invalid("The manifest needs a name and a version.");
    };
    let default_locale = text(&value, "default_locale");
    let locale = default_locale.as_deref();
    let name = localized(name, locale, read);
    let description = localized(
        text(&value, "description").unwrap_or_default(),
        locale,
        read,
    );

    let list = |key: &str| -> Vec<String> {
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let is_host = |pattern: &str| pattern == "<all_urls>" || pattern.contains("://");
    let mut permissions = Vec::new();
    let mut host_patterns = Vec::new();
    // Manifest 2 lists hosts among the permissions.
    for item in list("permissions")
        .into_iter()
        .chain(list("optional_permissions"))
    {
        if is_host(&item) {
            host_patterns.push(item);
        } else {
            permissions.push(item);
        }
    }
    host_patterns.extend(list("host_permissions"));
    if let Some(scripts) = value
        .get("content_scripts")
        .and_then(serde_json::Value::as_array)
    {
        for script in scripts {
            if let Some(matches) = script.get("matches").and_then(serde_json::Value::as_array) {
                host_patterns.extend(matches.iter().filter_map(|m| m.as_str().map(str::to_owned)));
            }
        }
    }
    permissions.sort();
    permissions.dedup();
    host_patterns.sort();
    host_patterns.dedup();

    let action = ["action", "browser_action", "page_action"]
        .iter()
        .find_map(|key| value.get(key));
    let popup = action
        .and_then(|action| text(action, "default_popup"))
        .filter(|path| is_safe_relative_path(path));
    let action_title = action
        .and_then(|action| text(action, "default_title"))
        .map(|title| localized(title, locale, read));
    let icon = value.get("icons").and_then(largest_icon).or_else(|| {
        action
            .and_then(|action| action.get("default_icon"))
            .and_then(largest_icon)
    });

    let all_sites = host_patterns.iter().any(|pattern| is_all_sites(pattern));
    let warnings = warnings(&permissions, &host_patterns);
    Ok(Summary {
        name,
        version,
        description,
        manifest_version,
        permissions,
        host_patterns,
        popup,
        icon,
        action_title,
        key: text(&value, "key"),
        warnings,
        all_sites,
    })
}

// -- Permissions -------------------------------------------------------------------

/// Whether a host pattern covers every website.
pub fn is_all_sites(pattern: &str) -> bool {
    matches!(
        pattern,
        "<all_urls>"
            | "*://*/*"
            | "http://*/*"
            | "https://*/*"
            | "*://*/"
            | "http://*/"
            | "https://*/"
    )
}

/// The site a host pattern names, such as `example.com` for `https://*.example.com/*`.
fn host_of(pattern: &str) -> Option<String> {
    let rest = pattern.split_once("://")?.1;
    let host = rest.split('/').next()?.trim_start_matches("*.");
    (!host.is_empty() && host != "*").then(|| host.to_owned())
}

fn permission_warning(permission: &str) -> Option<&'static str> {
    Some(match permission {
        "tabs" => "See the addresses of the sites you visit",
        "history" => "Read and change your browsing history",
        "bookmarks" => "Read and change your bookmarks",
        "cookies" => "Read and change your cookies",
        "downloads" => "Manage your downloads",
        "webRequest" | "webRequestBlocking" | "webRequestAuthProvider" => {
            "See and change the network requests you make"
        }
        "declarativeNetRequest" | "declarativeNetRequestWithHostAccess" => {
            "Block content on pages you visit"
        }
        "declarativeNetRequestFeedback" => "See which pages had content blocked",
        "notifications" => "Show notifications",
        "clipboardRead" => "Read data you copy and paste",
        "clipboardWrite" => "Change data you copy and paste",
        "geolocation" => "Know your location",
        "nativeMessaging" => "Communicate with other programs on your computer",
        "management" => "Manage your other extensions",
        "proxy" => "Control how you connect to the internet",
        "debugger" => "Read and change everything on every page, through the page debugger",
        "privacy" => "Change your privacy settings",
        "topSites" => "See the sites you visit most",
        "browsingData" => "Clear your browsing data",
        "contentSettings" => "Change what sites may do, such as use scripts",
        "desktopCapture" => "Record your screen",
        "tabCapture" | "pageCapture" => "Capture what's on pages you visit",
        "webNavigation" => "See which pages you open and move between",
        "identity" => "Know the account you're signed in with",
        "enterprise.deviceAttributes" | "enterprise.platformKeys" => "Read device details",
        "system.storage" | "system.cpu" | "system.memory" | "system.display" => {
            "Read details of your computer"
        }
        "readingList" => "Read and change your reading list",
        "sidePanel" => "Show a panel beside pages",
        "offscreen" => "Open hidden pages in the background",
        _ => return None,
    })
}

/// What an extension can do, in words, with the most sweeping things first.
pub fn warnings(permissions: &[String], host_patterns: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |text: String| {
        if !out.contains(&text) {
            out.push(text);
        }
    };
    if host_patterns.iter().any(|pattern| is_all_sites(pattern)) {
        add("Read and change all your data on all websites".into());
    } else {
        let mut hosts: Vec<String> = host_patterns.iter().filter_map(|p| host_of(p)).collect();
        hosts.sort();
        hosts.dedup();
        match hosts.len() {
            0 => {}
            1..=3 => add(format!("Read and change your data on {}", hosts.join(", "))),
            count => add(format!("Read and change your data on {count} websites")),
        }
    }
    for permission in permissions {
        if let Some(text) = permission_warning(permission) {
            add(text.to_owned());
        }
    }
    out
}

// -- IDs ---------------------------------------------------------------------------

fn id_from_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(16)
        .flat_map(|byte| [byte >> 4, byte & 0x0F])
        .map(|nibble| char::from(b'a' + nibble))
        .collect()
}

/// The ID Chromium gives an extension loaded from a folder, which comes from the
/// folder's full path.
pub fn id_for_path(path: &Path) -> String {
    id_from_bytes(path.to_string_lossy().as_bytes())
}

/// The ID of an extension whose manifest has a `key`.
pub fn id_for_key(key: &str) -> Option<String> {
    let compact: String = key.split_whitespace().collect();
    BASE64.decode(compact).ok().map(|der| id_from_bytes(&der))
}

fn is_valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
}

fn is_valid_folder(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

// -- Unpacking ---------------------------------------------------------------------

/// The ZIP inside a `.crx` file, which wraps it in a header holding signatures.
pub fn crx_to_zip(bytes: &[u8]) -> Result<&[u8]> {
    let word = |at: usize| -> Option<usize> {
        let part: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        Some(u32::from_le_bytes(part) as usize)
    };
    if bytes.get(..4) != Some(b"Cr24") {
        return invalid("This isn't a .crx file.");
    }
    let start = match word(4) {
        Some(2) => word(8)
            .zip(word(12))
            .map(|(key, signature)| 16 + key + signature),
        Some(3) => word(8).map(|header| 12 + header),
        _ => return invalid("This .crx file is in a format Stardust doesn't know."),
    };
    match start.filter(|start| *start < bytes.len()) {
        Some(start) => Ok(&bytes[start..]),
        None => invalid("This .crx file is damaged."),
    }
}

/// Unpacks a ZIP into `dest`. Files outside it, links, and packages that would unpack
/// into too much are refused.
pub fn unpack_zip(archive: &[u8], dest: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive))
        .map_err(|_| Error::Invalid("This isn't a valid .zip file.".into()))?;
    if archive.len() > MAX_FILES {
        return invalid("This package has too many files.");
    }
    fs::create_dir_all(dest)?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| Error::Invalid("The .zip file is damaged.".into()))?;
        let Some(relative) = entry.enclosed_name() else {
            return invalid("The package has a file that points outside itself.");
        };
        // The high bits of a ZIP entry's mode say what kind of file it is.
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return invalid("The package contains a link, which isn't allowed.");
        }
        let target = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&target)?;
        let written = std::io::copy(&mut (&mut entry).take(MAX_PACKAGE_BYTES + 1), &mut file)?;
        total += written;
        if total > MAX_PACKAGE_BYTES {
            return invalid("This package is too large once unpacked.");
        }
    }
    Ok(())
}

/// Copies a folder, skipping links, with the same limits as unpacking.
fn copy_folder(
    source: &Path,
    dest: &Path,
    depth: usize,
    files: &mut usize,
    total: &mut u64,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return invalid("The folder is nested too deeply.");
    }
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name();
        if kind.is_symlink() || name == ".git" {
            continue;
        }
        let target = dest.join(&name);
        if kind.is_dir() {
            copy_folder(&entry.path(), &target, depth + 1, files, total)?;
        } else {
            *files += 1;
            *total += entry.metadata()?.len();
            if *files > MAX_FILES || *total > MAX_PACKAGE_BYTES {
                return invalid("This folder is too large to be an extension.");
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The folder holding `manifest.json`: `root` itself, or the one folder inside it that
/// does, as zipping a folder produces.
fn manifest_root(root: &Path) -> Result<PathBuf> {
    if root.join("manifest.json").is_file() {
        return Ok(root.to_owned());
    }
    let mut folders = fs::read_dir(root)?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path());
    match (folders.next(), folders.next()) {
        (Some(only), None) if only.join("manifest.json").is_file() => Ok(only),
        _ => invalid("There's no manifest.json in this package."),
    }
}

/// An extension unpacked and checked, waiting for the person to agree to add it.
#[derive(Debug)]
pub struct Prepared {
    /// Where it was unpacked. Removed once installed or dropped.
    pub staging: PathBuf,
    /// The folder with `manifest.json`.
    pub root: PathBuf,
    pub summary: Summary,
}

fn read_relative(root: &Path, relative: &str, limit: u64) -> Option<Vec<u8>> {
    if !is_safe_relative_path(relative) {
        return None;
    }
    let path = root.join(relative);
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

/// Reads a manifest from a folder.
pub fn summarize(root: &Path) -> Result<Summary> {
    let manifest = read_relative(root, "manifest.json", MAX_MANIFEST_BYTES + 1)
        .ok_or_else(|| Error::Invalid("There's no manifest.json in this package.".into()))?;
    parse_manifest(&manifest, &|path| {
        read_relative(root, path, MAX_MANIFEST_BYTES)
    })
}

/// Unpacks a folder, `.zip` or `.crx` into `staging` and reads its manifest.
pub fn prepare(source: &Path, staging: &Path) -> Result<Prepared> {
    let _ = fs::remove_dir_all(staging);
    let result = (|| {
        if source.is_dir() {
            copy_folder(source, staging, 0, &mut 0, &mut 0)?;
        } else {
            let mut bytes = Vec::new();
            fs::File::open(source)?
                .take(MAX_PACKAGE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_PACKAGE_BYTES {
                return invalid("This file is too large to be an extension.");
            }
            let archive = if bytes.starts_with(b"Cr24") {
                crx_to_zip(&bytes)?
            } else {
                &bytes
            };
            unpack_zip(archive, staging)?;
        }
        let root = manifest_root(staging)?;
        let summary = summarize(&root)?;
        Ok(Prepared {
            staging: staging.to_owned(),
            root,
            summary,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

// -- The installed list ------------------------------------------------------------

/// One installed extension, as remembered between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    /// Its folder's name inside the extensions folder.
    pub folder: String,
    pub enabled: bool,
    #[serde(default)]
    pub pinned: bool,
    pub installed_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    entries: Vec<Entry>,
}

/// An installed extension with its manifest and icon, for showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub id: String,
    pub enabled: bool,
    pub pinned: bool,
    #[serde(flatten)]
    pub summary: Summary,
    /// The icon as a `data:` address, so pages can show it without file access.
    pub icon_data: Option<String>,
    /// The popup's address, when the extension has one.
    pub popup_url: Option<String>,
}

impl Registry {
    /// Reads the saved list. Entries edited into something unsafe are dropped.
    pub fn parse(json: &str) -> Self {
        let mut registry: Self = serde_json::from_str(json).unwrap_or_default();
        registry
            .entries
            .retain(|entry| is_valid_id(&entry.id) && is_valid_folder(&entry.folder));
        let mut seen = std::collections::HashSet::new();
        registry
            .entries
            .retain(|entry| seen.insert(entry.id.clone()));
        registry
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to save the extensions list")
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn add(&mut self, entry: Entry) {
        self.entries.retain(|existing| existing.id != entry.id);
        self.entries.push(entry);
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| entry.enabled = enabled)
            .is_some()
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> bool {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| entry.pinned = pinned)
            .is_some()
    }

    pub fn remove(&mut self, id: &str) -> Option<Entry> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(index))
    }

    /// The folders Chromium should load: enabled extensions that are still on disk.
    pub fn load_paths(&self, extensions_dir: &Path) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|entry| entry.enabled)
            .filter_map(|entry| fs::canonicalize(extensions_dir.join(&entry.folder)).ok())
            .filter(|path| path.join("manifest.json").is_file())
            .collect()
    }

    /// Everything the hub shows. Extensions whose files have gone are left out.
    pub fn describe(&self, extensions_dir: &Path) -> Vec<Info> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let root = extensions_dir.join(&entry.folder);
                let summary = summarize(&root).ok()?;
                let icon_data = summary
                    .icon
                    .as_deref()
                    .and_then(|icon| icon_data_url(&root, icon));
                let popup_url = summary
                    .popup
                    .as_ref()
                    .map(|popup| format!("chrome-extension://{}/{popup}", entry.id));
                Some(Info {
                    id: entry.id.clone(),
                    enabled: entry.enabled,
                    pinned: entry.pinned,
                    summary,
                    icon_data,
                    popup_url,
                })
            })
            .collect()
    }
}

/// An icon file as a `data:` address, if it's a picture type that can't hold scripts.
pub fn icon_data_url(root: &Path, icon: &str) -> Option<String> {
    let mime = match Path::new(icon)
        .extension()?
        .to_str()?
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        // SVG can hold scripts, and the pages showing icons shouldn't take that chance.
        _ => return None,
    };
    let bytes = read_relative(root, icon, MAX_ICON_BYTES + 1)?;
    (bytes.len() as u64 <= MAX_ICON_BYTES)
        .then(|| format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

/// Moves an unpacked extension into its own folder under `extensions_dir`, and returns
/// what to remember about it. The folder's name is random, and the ID comes from where
/// it ends up, as Chromium works it out.
pub fn install(
    prepared: Prepared,
    extensions_dir: &Path,
    now_ms: i64,
    seed: &str,
) -> Result<Entry> {
    fs::create_dir_all(extensions_dir)?;
    let digest = Sha256::digest(format!("{seed}{now_ms}{}", prepared.summary.name));
    let folder: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let target = extensions_dir.join(&folder);
    fs::rename(&prepared.root, &target)?;
    let _ = fs::remove_dir_all(&prepared.staging);
    let id = prepared
        .summary
        .key
        .as_deref()
        .and_then(id_for_key)
        .or_else(|| {
            fs::canonicalize(&target)
                .ok()
                .map(|path| id_for_path(&path))
        })
        .ok_or_else(|| Error::Invalid("Couldn't work out the extension's ID.".into()))?;
    Ok(Entry {
        id,
        folder,
        enabled: true,
        pinned: false,
        installed_ms: now_ms,
    })
}

/// Deletes an installed extension's files.
pub fn delete_files(entry: &Entry, extensions_dir: &Path) {
    if is_valid_folder(&entry.folder) {
        let _ = fs::remove_dir_all(extensions_dir.join(&entry.folder));
    }
}

/// The extensions folder's contents by name, for tests and tidying.
pub fn folders(extensions_dir: &Path) -> BTreeMap<String, PathBuf> {
    fs::read_dir(extensions_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn none(_: &str) -> Option<Vec<u8>> {
        None
    }

    fn temp(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("stardust-ext-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (path, data) in files {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const MV3: &str = r#"{
        "manifest_version": 3, "name": "Blocker", "version": "1.2.3",
        "description": "Blocks things.",
        "permissions": ["storage", "declarativeNetRequest", "cookies"],
        "host_permissions": ["https://*.example.com/*"],
        "action": {"default_popup": "popup.html", "default_title": "Blocker"},
        "icons": {"16": "i16.png", "48": "i48.png", "256": "i256.png"},
        "content_scripts": [{"matches": ["https://other.test/*"], "js": ["c.js"]}]
    }"#;

    #[test]
    fn reads_a_manifest_three_extension() {
        let summary = parse_manifest(MV3.as_bytes(), &none).unwrap();
        assert_eq!(summary.name, "Blocker");
        assert_eq!(summary.version, "1.2.3");
        assert_eq!(summary.popup.as_deref(), Some("popup.html"));
        // The icon is the largest that isn't enormous.
        assert_eq!(summary.icon.as_deref(), Some("i48.png"));
        assert_eq!(
            summary.permissions,
            ["cookies", "declarativeNetRequest", "storage"]
        );
        assert_eq!(
            summary.host_patterns,
            ["https://*.example.com/*", "https://other.test/*"]
        );
        assert!(!summary.all_sites);
    }

    #[test]
    fn manifest_two_puts_hosts_among_permissions() {
        let json = r#"{"manifest_version": 2, "name": "Old", "version": "1",
            "permissions": ["tabs", "<all_urls>"], "browser_action": {"default_popup": "p.html"}}"#;
        let summary = parse_manifest(json.as_bytes(), &none).unwrap();
        assert_eq!(summary.permissions, ["tabs"]);
        assert!(summary.all_sites);
        assert_eq!(summary.popup.as_deref(), Some("p.html"));
        assert_eq!(
            summary.warnings[0],
            "Read and change all your data on all websites"
        );
    }

    #[test]
    fn refuses_things_that_arent_extensions() {
        assert!(parse_manifest(b"not json", &none).is_err());
        assert!(parse_manifest(br#"{"name":"x","version":"1"}"#, &none).is_err());
        assert!(parse_manifest(br#"{"manifest_version":3,"name":"x"}"#, &none).is_err());
        assert!(
            parse_manifest(br#"{"manifest_version":1,"name":"x","version":"1"}"#, &none).is_err()
        );
        // A byte order mark is fine.
        let mut with_mark = b"\xEF\xBB\xBF".to_vec();
        with_mark.extend_from_slice(MV3.as_bytes());
        assert!(parse_manifest(&with_mark, &none).is_ok());
    }

    #[test]
    fn uses_the_extensions_own_translations() {
        let json = r#"{"manifest_version":3,"name":"__MSG_appName__","version":"1","default_locale":"en"}"#;
        let read = |path: &str| {
            (path == "_locales/en/messages.json")
                .then(|| br#"{"APPNAME": {"message": "Real name"}}"#.to_vec())
        };
        assert_eq!(
            parse_manifest(json.as_bytes(), &read).unwrap().name,
            "Real name"
        );
        assert_eq!(
            parse_manifest(json.as_bytes(), &none).unwrap().name,
            "__MSG_appName__"
        );
    }

    #[test]
    fn icons_and_popups_cant_point_outside_the_folder() {
        for bad in [
            "../x.png",
            "/etc/passwd",
            "a/../../b",
            "https://evil.test/x.png",
            "a\\b",
            "",
        ] {
            assert!(!is_safe_relative_path(bad), "{bad}");
        }
        assert!(is_safe_relative_path("images/icon.png"));
        let json = r#"{"manifest_version":3,"name":"x","version":"1","icons":{"48":"../../secret.png"},
            "action":{"default_popup":"https://evil.test/p.html"}}"#;
        let summary = parse_manifest(json.as_bytes(), &none).unwrap();
        assert_eq!((summary.icon, summary.popup), (None, None));
    }

    #[test]
    fn warns_in_plain_words() {
        let all = warnings(
            &["cookies".into(), "storage".into()],
            &["<all_urls>".into()],
        );
        assert_eq!(all[0], "Read and change all your data on all websites");
        assert!(all.contains(&"Read and change your cookies".to_string()));
        // Storage needs no warning.
        assert_eq!(all.len(), 2);
        let one = warnings(&[], &["https://*.example.com/*".into()]);
        assert_eq!(one, ["Read and change your data on example.com"]);
        let many = warnings(
            &[],
            &[
                "https://a.test/*".into(),
                "https://b.test/*".into(),
                "https://c.test/*".into(),
                "https://d.test/*".into(),
            ],
        );
        assert_eq!(many, ["Read and change your data on 4 websites"]);
    }

    #[test]
    fn ids_match_the_ones_chromium_gave_a_real_folder() {
        // Measured in this project's own test: Chromium loaded this folder as this ID.
        let path = Path::new(
            "/private/tmp/claude-501/-Users-johnwesleygovada-Desktop-Whatsapp-stardust/e14e8cf0-a9ea-4a8a-85ca-164aad8cc30a/scratchpad/ext-test",
        );
        assert_eq!(id_for_path(path), "dkagmhlkifoekndepikohldifcmfknjl");
        assert!(is_valid_id(&id_for_path(Path::new("/x"))));
        assert!(!is_valid_id("short"));
        assert!(!is_valid_id(&"z".repeat(32)));
    }

    #[test]
    fn unwraps_crx_headers() {
        let archive = zip_of(&[("manifest.json", MV3.as_bytes())]);
        let mut v3 = b"Cr24".to_vec();
        v3.extend_from_slice(&3u32.to_le_bytes());
        v3.extend_from_slice(&5u32.to_le_bytes());
        v3.extend_from_slice(b"12345");
        v3.extend_from_slice(&archive);
        assert_eq!(crx_to_zip(&v3).unwrap(), archive.as_slice());

        let mut v2 = b"Cr24".to_vec();
        v2.extend_from_slice(&2u32.to_le_bytes());
        v2.extend_from_slice(&2u32.to_le_bytes());
        v2.extend_from_slice(&3u32.to_le_bytes());
        v2.extend_from_slice(b"kksss");
        v2.extend_from_slice(&archive);
        assert_eq!(crx_to_zip(&v2).unwrap(), archive.as_slice());

        assert!(crx_to_zip(b"PK\x03\x04").is_err());
        assert!(crx_to_zip(b"Cr24\x09\0\0\0").is_err());
        assert!(crx_to_zip(b"Cr24\x03\0\0\0\xff\xff\xff\x7f").is_err());
    }

    #[test]
    fn unpacks_a_zip_and_finds_the_manifest_in_a_folder() {
        let root = temp("unpack");
        let archive = zip_of(&[
            ("my-ext/manifest.json", MV3.as_bytes()),
            ("my-ext/popup.html", b"<p>"),
        ]);
        let file = root.join("ext.zip");
        fs::write(&file, archive).unwrap();
        let prepared = prepare(&file, &root.join("staging")).unwrap();
        assert_eq!(prepared.summary.name, "Blocker");
        assert!(prepared.root.ends_with("my-ext"));
        assert!(prepared.root.join("popup.html").is_file());
    }

    #[test]
    fn refuses_files_that_escape_the_folder() {
        let root = temp("escape");
        let archive = zip_of(&[("manifest.json", MV3.as_bytes()), ("../evil.txt", b"x")]);
        let file = root.join("ext.zip");
        fs::write(&file, archive).unwrap();
        assert!(prepare(&file, &root.join("staging")).is_err());
        assert!(!root.join("evil.txt").exists());
        // A failed install leaves nothing behind.
        assert!(!root.join("staging").exists());
    }

    #[test]
    fn refuses_packages_without_a_manifest() {
        let root = temp("empty");
        let file = root.join("ext.zip");
        fs::write(&file, zip_of(&[("readme.txt", b"hi")])).unwrap();
        assert!(prepare(&file, &root.join("staging")).is_err());
        assert!(prepare(&root.join("missing.zip"), &root.join("staging")).is_err());
    }

    #[test]
    fn installs_from_a_folder_and_lists_it() {
        let root = temp("install");
        let source = root.join("source");
        fs::create_dir_all(source.join("img")).unwrap();
        fs::write(source.join("manifest.json"), MV3).unwrap();
        fs::write(source.join("popup.html"), "<p>").unwrap();
        fs::write(source.join("img/i48.png"), b"\x89PNG").unwrap();

        let prepared = prepare(&source, &root.join("staging")).unwrap();
        let extensions = root.join("Extensions");
        let entry = install(prepared, &extensions, 1000, "seed").unwrap();
        assert!(is_valid_id(&entry.id));
        assert!(
            extensions
                .join(&entry.folder)
                .join("manifest.json")
                .is_file()
        );
        assert!(!root.join("staging").exists());
        // The ID is the one Chromium works out for the folder it will load.
        let loaded = fs::canonicalize(extensions.join(&entry.folder)).unwrap();
        assert_eq!(entry.id, id_for_path(&loaded));

        let mut registry = Registry::default();
        registry.add(entry.clone());
        assert_eq!(registry.load_paths(&extensions), [loaded]);
        let listed = registry.describe(&extensions);
        assert_eq!(listed[0].summary.name, "Blocker");
        assert_eq!(
            listed[0].popup_url.as_deref(),
            Some(format!("chrome-extension://{}/popup.html", entry.id).as_str())
        );

        // Turning it off keeps it listed but stops it loading.
        assert!(registry.set_enabled(&entry.id, false));
        assert!(registry.load_paths(&extensions).is_empty());
        assert_eq!(registry.describe(&extensions).len(), 1);
        let removed = registry.remove(&entry.id).unwrap();
        delete_files(&removed, &extensions);
        assert!(folders(&extensions).is_empty());
    }

    #[test]
    fn icons_become_data_addresses_but_not_svg() {
        let root = temp("icons");
        fs::write(root.join("a.png"), b"\x89PNG").unwrap();
        fs::write(root.join("a.svg"), b"<svg onload=alert(1)>").unwrap();
        assert!(
            icon_data_url(&root, "a.png")
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(icon_data_url(&root, "a.svg"), None);
        assert_eq!(icon_data_url(&root, "../a.png"), None);
        assert_eq!(icon_data_url(&root, "missing.png"), None);
    }

    #[test]
    fn edited_lists_cant_name_unsafe_folders() {
        let good = format!(
            r#"{{"id":"{}","folder":"abc-123","enabled":true,"installed_ms":1}}"#,
            "a".repeat(32)
        );
        let traversal = format!(
            r#"{{"id":"{}","folder":"../../home","enabled":true,"installed_ms":1}}"#,
            "b".repeat(32)
        );
        let bad_id = r#"{"id":"nope","folder":"ok","enabled":true,"installed_ms":1}"#;
        let json = format!(r#"{{"entries":[{good},{traversal},{bad_id},{good}]}}"#);
        let registry = Registry::parse(&json);
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.entries()[0].folder, "abc-123");
        assert_eq!(Registry::parse("garbage"), Registry::default());
        // Deleting with a bad folder name does nothing.
        let outside = temp("outside");
        fs::write(outside.join("keep"), "x").unwrap();
        delete_files(
            &Entry {
                id: "a".repeat(32),
                folder: "../".into(),
                enabled: true,
                pinned: false,
                installed_ms: 0,
            },
            &outside.join("Extensions"),
        );
        assert!(outside.join("keep").exists());
    }
}
