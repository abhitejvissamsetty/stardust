//! Finds, checks and installs new versions of Stardust.
//!
//! A release server holds `manifest.json`, which names the newest version and one
//! package per platform, and `manifest.json.sig`, an Ed25519 signature of it. Only a
//! manifest signed by the key in `release-key.pub` is believed, and a package is only
//! used if its size and SHA-256 match the signed manifest. So a server, network or
//! download that's been tampered with can't install anything.
//!
//! A new version is downloaded and unpacked into a staging folder while the browser
//! runs, then swapped in at the next start, before Chromium loads.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The public half of the key that signs releases.
const RELEASE_KEY: &str = include_str!("../release-key.pub");
/// Larger downloads are refused, so a bad manifest can't fill the disk.
const MAX_PACKAGE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const READY_FILE: &str = "ready";

#[derive(Debug)]
pub enum Error {
    Network(String),
    BadSignature,
    BadManifest(String),
    BadPackage(String),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(error) => write!(f, "couldn't reach the update server: {error}"),
            Self::BadSignature => write!(f, "the update list isn't signed by Stardust's key"),
            Self::BadManifest(error) => write!(f, "the update list is damaged: {error}"),
            Self::BadPackage(error) => write!(f, "the download is damaged: {error}"),
            Self::Io(error) => write!(f, "couldn't save the update: {error}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// The signed list of releases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Packages by platform, such as `macos-aarch64`.
    pub packages: std::collections::BTreeMap<String, Package>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    /// Relative to the manifest's address, or a full address.
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// A newer version for this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// This computer's platform, as manifests name it.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether version `a` is newer than `b`. Versions are dot-separated numbers.
pub fn is_newer(a: &str, b: &str) -> bool {
    let parts = |version: &str| -> Option<Vec<u64>> {
        version.split('.').map(|part| part.parse().ok()).collect()
    };
    match (parts(a), parts(b)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

fn release_key() -> Result<VerifyingKey> {
    let bytes = BASE64
        .decode(RELEASE_KEY.trim())
        .map_err(|_| Error::BadSignature)?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| Error::BadSignature)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| Error::BadSignature)
}

/// Checks a manifest's signature and reads it.
pub fn verify_manifest(manifest: &[u8], signature: &str, key: &VerifyingKey) -> Result<Manifest> {
    let signature = BASE64
        .decode(signature.trim())
        .map_err(|_| Error::BadSignature)?;
    let signature = Signature::from_slice(&signature).map_err(|_| Error::BadSignature)?;
    key.verify(manifest, &signature)
        .map_err(|_| Error::BadSignature)?;
    serde_json::from_slice(manifest).map_err(|error| Error::BadManifest(error.to_string()))
}

/// The release in a manifest worth installing over `current`, if any.
pub fn newer_release(
    manifest: &Manifest,
    current: &str,
    platform: &str,
    manifest_url: &str,
) -> Result<Option<Release>> {
    if !is_newer(&manifest.version, current) {
        return Ok(None);
    }
    let Some(package) = manifest.packages.get(platform) else {
        return Ok(None);
    };
    let url = join_url(manifest_url, &package.url)?;
    let is_hex =
        package.sha256.len() == 64 && package.sha256.bytes().all(|b| b.is_ascii_hexdigit());
    if !is_hex || package.size == 0 || package.size > MAX_PACKAGE_BYTES {
        return Err(Error::BadManifest("invalid package details".into()));
    }
    Ok(Some(Release {
        version: manifest.version.clone(),
        url,
        size: package.size,
        sha256: package.sha256.to_ascii_lowercase(),
    }))
}

/// Resolves a package address against the manifest's, and checks it's safe to fetch.
fn join_url(manifest_url: &str, package_url: &str) -> Result<String> {
    let url = if package_url.contains("://") {
        package_url.to_owned()
    } else {
        let base = manifest_url.rsplit_once('/').map_or("", |(base, _)| base);
        format!("{base}/{}", package_url.trim_start_matches('/'))
    };
    if is_allowed_url(&url) {
        Ok(url)
    } else {
        Err(Error::BadManifest(format!("refused address {url}")))
    }
}

/// Updates come over HTTPS. Plain HTTP is only allowed to this computer, for testing.
pub fn is_allowed_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    let Some(rest) = lower.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    host == "127.0.0.1" || host == "localhost"
}

fn fetch(url: &str, limit: u64) -> Result<Vec<u8>> {
    if !is_allowed_url(url) {
        return Err(Error::Network(format!("refused address {url}")));
    }
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|error| Error::Network(error.to_string()))?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|error| Error::Network(error.to_string()))?;
    if body.len() as u64 > limit {
        return Err(Error::Network("the reply is too large".into()));
    }
    Ok(body)
}

/// Asks the server for a newer version. `manifest_url` is the full address of
/// `manifest.json`.
pub fn check(manifest_url: &str, current: &str) -> Result<Option<Release>> {
    let manifest = fetch(manifest_url, MAX_MANIFEST_BYTES)?;
    let signature = fetch(&format!("{manifest_url}.sig"), 1024)?;
    let signature = String::from_utf8(signature).map_err(|_| Error::BadSignature)?;
    let manifest = verify_manifest(&manifest, &signature, &release_key()?)?;
    newer_release(&manifest, current, &platform(), manifest_url)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Downloads a release, checks it, and unpacks it into `staging/<version>`, ready to
/// install at the next start.
pub fn download(release: &Release, staging: &Path) -> Result<()> {
    let package = fetch(&release.url, release.size)?;
    stage(release, &package, staging)
}

/// Checks a downloaded package against its release and unpacks it.
pub fn stage(release: &Release, package: &[u8], staging: &Path) -> Result<()> {
    if package.len() as u64 != release.size {
        return Err(Error::BadPackage("wrong size".into()));
    }
    if sha256_hex(package) != release.sha256 {
        return Err(Error::BadPackage("wrong checksum".into()));
    }
    // Older staged versions are no longer needed.
    if staging.exists() {
        fs::remove_dir_all(staging)?;
    }
    let folder = staging.join(&release.version);
    let payload = folder.join("payload");
    fs::create_dir_all(&payload)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(package));
    archive.set_preserve_permissions(true);
    for entry in archive.entries()? {
        let mut entry = entry?;
        // Paths and links must stay inside the folder. `unpack_in` refuses paths
        // with `..`, and links are checked here.
        if let Some(target) = entry.link_name()? {
            let escapes = target.is_absolute()
                || target
                    .components()
                    .filter(|part| matches!(part, std::path::Component::ParentDir))
                    .count()
                    > entry.path()?.components().count().saturating_sub(1);
            if escapes {
                return Err(Error::BadPackage(
                    "a link points outside the package".into(),
                ));
            }
        }
        if !entry.unpack_in(&payload)? {
            return Err(Error::BadPackage(
                "a path points outside the package".into(),
            ));
        }
    }
    fs::File::create(folder.join(READY_FILE))?.write_all(release.version.as_bytes())?;
    Ok(())
}

/// The newest staged version, if it's complete and newer than `current`.
pub fn staged_version(staging: &Path, current: &str) -> Option<(String, PathBuf)> {
    fs::read_dir(staging)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let version = fs::read_to_string(entry.path().join(READY_FILE)).ok()?;
            let named = entry.file_name().to_string_lossy() == version;
            (named && is_newer(&version, current)).then(|| (version, entry.path().join("payload")))
        })
        .max_by(|(a, _), (b, _)| {
            if is_newer(a, b) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        })
}

/// The folder an update replaces: the `.app` bundle on macOS, or the folder holding
/// the executable elsewhere.
pub fn install_folder(executable: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let bundle = executable.parent()?.parent()?.parent()?;
        (bundle.extension()? == "app").then(|| bundle.to_path_buf())
    } else {
        executable.parent().map(Path::to_path_buf)
    }
}

/// The folder in an unpacked package that replaces the install folder. On macOS it's the
/// `.app` bundle, which has a fixed name. Elsewhere people install into a folder of any
/// name, so it's the package's one top-level folder, whatever it's called.
fn package_folder(payload: &Path, install_name: &std::ffi::OsStr) -> Result<PathBuf> {
    if cfg!(target_os = "macos") {
        let new = payload.join(install_name);
        return if new.exists() {
            Ok(new)
        } else {
            Err(Error::BadPackage(format!(
                "the package has no {}",
                install_name.to_string_lossy()
            )))
        };
    }
    let mut items = fs::read_dir(payload)?.collect::<std::io::Result<Vec<_>>>()?;
    match (items.pop(), items.is_empty()) {
        (Some(only), true) if only.file_type()?.is_dir() => Ok(only.path()),
        _ => Err(Error::BadPackage(
            "the package should hold one folder".into(),
        )),
    }
}

/// Swaps a staged version in for the installed one. Returns the new install folder's
/// executable, to restart into, or nothing if there was nothing to install.
///
/// The old version is kept beside it as `<folder>.old` until the next update, so a
/// failed swap can be put back.
pub fn install_staged(executable: &Path, staging: &Path, current: &str) -> Result<Option<PathBuf>> {
    let Some((_, payload)) = staged_version(staging, current) else {
        return Ok(None);
    };
    let Some(install) = install_folder(executable) else {
        return Ok(None);
    };
    let name = install
        .file_name()
        .ok_or_else(|| Error::BadPackage("no install folder".into()))?;
    let new = package_folder(&payload, name)?;
    let old = install.with_file_name(format!("{}.old", name.to_string_lossy()));
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    fs::rename(&install, &old)?;
    if let Err(error) = fs::rename(&new, &install) {
        // Put the old version back, so the browser still starts.
        fs::rename(&old, &install)?;
        return Err(error.into());
    }
    let _ = fs::remove_dir_all(staging);
    let relative = executable.strip_prefix(&install).unwrap_or(executable);
    Ok(Some(install.join(relative)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn manifest() -> Manifest {
        let mut packages = std::collections::BTreeMap::new();
        packages.insert(
            "macos-aarch64".into(),
            Package {
                url: "stardust-0.2.0.tar.gz".into(),
                size: 10,
                sha256: "a".repeat(64),
            },
        );
        Manifest {
            version: "0.2.0".into(),
            packages,
        }
    }

    fn signed(manifest: &Manifest) -> (Vec<u8>, String) {
        let bytes = serde_json::to_vec(manifest).unwrap();
        let signature = BASE64.encode(key().sign(&bytes).to_bytes());
        (bytes, signature)
    }

    #[test]
    fn compares_versions_by_number() {
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("x", "0.1.0"));
    }

    #[test]
    fn accepts_a_correctly_signed_manifest() {
        let (bytes, signature) = signed(&manifest());
        let read = verify_manifest(&bytes, &signature, &key().verifying_key()).unwrap();
        assert_eq!(read, manifest());
    }

    #[test]
    fn refuses_a_changed_manifest() {
        let (mut bytes, signature) = signed(&manifest());
        let at = bytes.iter().position(|b| *b == b'2').unwrap();
        bytes[at] = b'9';
        assert!(matches!(
            verify_manifest(&bytes, &signature, &key().verifying_key()),
            Err(Error::BadSignature)
        ));
    }

    #[test]
    fn refuses_another_key() {
        let (bytes, signature) = signed(&manifest());
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key();
        assert!(verify_manifest(&bytes, &signature, &other).is_err());
    }

    #[test]
    fn only_offers_newer_versions_for_this_platform() {
        let url = "https://updates.example/stardust/manifest.json";
        let release = newer_release(&manifest(), "0.1.0", "macos-aarch64", url)
            .unwrap()
            .unwrap();
        assert_eq!(
            release.url,
            "https://updates.example/stardust/stardust-0.2.0.tar.gz"
        );
        assert_eq!(
            newer_release(&manifest(), "0.2.0", "macos-aarch64", url).unwrap(),
            None
        );
        assert_eq!(
            newer_release(&manifest(), "0.1.0", "linux-x86_64", url).unwrap(),
            None
        );
    }

    #[test]
    fn refuses_plain_http_except_to_this_computer() {
        assert!(is_allowed_url("https://a.example/x"));
        assert!(is_allowed_url("http://127.0.0.1:8000/x"));
        assert!(is_allowed_url("http://localhost/x"));
        assert!(!is_allowed_url("http://a.example/x"));
        assert!(!is_allowed_url("http://127.0.0.1.evil.example/x"));
        assert!(!is_allowed_url("file:///etc/passwd"));
        let mut manifest = manifest();
        manifest.packages.get_mut("macos-aarch64").unwrap().url = "http://a.example/x".into();
        assert!(newer_release(&manifest, "0.1.0", "macos-aarch64", "https://b.example/m").is_err());
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

    fn release_for(package: &[u8], version: &str) -> Release {
        Release {
            version: version.into(),
            url: String::new(),
            size: package.len() as u64,
            sha256: sha256_hex(package),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "stardust-update-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn stages_and_installs_a_checked_package() {
        let root = temp("install");
        let install = root.join("dist");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("stardust"), b"old").unwrap();

        let package = package(&[("dist/stardust", b"new")]);
        let staging = root.join("staging");
        stage(&release_for(&package, "0.2.0"), &package, &staging).unwrap();
        assert_eq!(staged_version(&staging, "0.1.0").unwrap().0, "0.2.0");

        let executable = install.join("stardust");
        if cfg!(target_os = "macos") {
            // macOS installs replace an `.app` bundle, covered by the end-to-end test.
            return;
        }
        let restarted = install_staged(&executable, &staging, "0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(&restarted).unwrap(), b"new");
        assert_eq!(fs::read(root.join("dist.old/stardust")).unwrap(), b"old");
        assert!(!staging.exists());
    }

    #[test]
    fn installs_into_a_folder_of_any_name() {
        if cfg!(target_os = "macos") {
            return;
        }
        let root = temp("any-name");
        let install = root.join("my browser");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("stardust"), b"old").unwrap();
        let package = package(&[("dist/stardust", b"new")]);
        let staging = root.join("staging");
        stage(&release_for(&package, "0.2.0"), &package, &staging).unwrap();

        let restarted = install_staged(&install.join("stardust"), &staging, "0.1.0")
            .unwrap()
            .unwrap();
        assert_eq!(restarted, install.join("stardust"));
        assert_eq!(fs::read(&restarted).unwrap(), b"new");
        assert_eq!(
            fs::read(root.join("my browser.old/stardust")).unwrap(),
            b"old"
        );
    }

    #[test]
    fn refuses_a_package_with_more_than_one_folder() {
        if cfg!(target_os = "macos") {
            return;
        }
        let root = temp("two-folders");
        let install = root.join("stardust");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join("stardust"), b"old").unwrap();
        let package = package(&[("dist/stardust", b"new"), ("other/file", b"x")]);
        let staging = root.join("staging");
        stage(&release_for(&package, "0.2.0"), &package, &staging).unwrap();

        assert!(matches!(
            install_staged(&install.join("stardust"), &staging, "0.1.0"),
            Err(Error::BadPackage(_))
        ));
        assert_eq!(fs::read(install.join("stardust")).unwrap(), b"old");
    }

    #[test]
    fn refuses_a_package_that_doesnt_match() {
        let root = temp("mismatch");
        let package = package(&[("dist/stardust", b"new")]);
        let mut release = release_for(&package, "0.2.0");
        release.sha256 = "0".repeat(64);
        assert!(matches!(
            stage(&release, &package, &root.join("staging")),
            Err(Error::BadPackage(_))
        ));
        release = release_for(&package, "0.2.0");
        release.size += 1;
        assert!(stage(&release, &package, &root.join("staging")).is_err());
    }

    #[test]
    fn ignores_staged_versions_that_arent_newer() {
        let root = temp("older");
        let package = package(&[("dist/stardust", b"x")]);
        let staging = root.join("staging");
        stage(&release_for(&package, "0.1.0"), &package, &staging).unwrap();
        assert_eq!(staged_version(&staging, "0.1.0"), None);
    }

    #[test]
    fn refuses_links_out_of_the_package() {
        let root = temp("links");
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        builder
            .append_link(&mut header, "dist/evil", "../../../../etc/passwd")
            .unwrap();
        let package = builder.into_inner().unwrap().finish().unwrap();
        let result = stage(
            &release_for(&package, "0.2.0"),
            &package,
            &root.join("staging"),
        );
        assert!(matches!(result, Err(Error::BadPackage(_))));
    }
}
