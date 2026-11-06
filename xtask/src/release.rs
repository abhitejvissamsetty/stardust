//! Release signing: making the key, and packaging and signing a release.

use std::fs;
use std::io::Write;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};

const PUBLIC_KEY_FILE: &str = "crates/update/release-key.pub";

/// Makes a signing key. The private key is written to `path` and readable only by
/// this user, and must never be committed. The public key goes into the updater.
pub fn keygen(path: &Path) {
    if path.exists() {
        eprintln!(
            "{} already exists. Remove it first to make a new key.",
            path.display()
        );
        std::process::exit(1);
    }
    if path
        .canonicalize()
        .or_else(|_| {
            path.parent()
                .map_or(Ok(Path::new(".").to_path_buf()), Path::canonicalize)
        })
        .is_ok_and(|full| full.starts_with(std::env::current_dir().unwrap_or_default()))
    {
        eprintln!("Keep the private key outside the repository, so it can't be committed.");
        std::process::exit(1);
    }
    let mut seed = [0; 32];
    getrandom::getrandom(&mut seed).expect("failed to get random bytes");
    let key = SigningKey::from_bytes(&seed);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create the key's folder");
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).expect("failed to write the private key");
    writeln!(file, "{}", BASE64.encode(seed)).expect("failed to write the private key");

    let public = BASE64.encode(key.verifying_key().to_bytes());
    fs::write(PUBLIC_KEY_FILE, format!("{public}\n")).expect("failed to write the public key");
    println!(
        "Private key: {} (keep it safe and out of the repository)",
        path.display()
    );
    println!("Public key: {public}, written to {PUBLIC_KEY_FILE}");
}

fn read_key(path: &Path) -> SigningKey {
    let text = fs::read_to_string(path).expect("failed to read the private key");
    let seed: [u8; 32] = BASE64
        .decode(text.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .expect("the private key file is damaged");
    let key = SigningKey::from_bytes(&seed);
    let public = BASE64.encode(key.verifying_key().to_bytes());
    let built_in = fs::read_to_string(PUBLIC_KEY_FILE).unwrap_or_default();
    if built_in.trim() != public {
        eprintln!(
            "This key doesn't match {PUBLIC_KEY_FILE}, so browsers wouldn't accept its releases."
        );
        std::process::exit(1);
    }
    key
}

/// The bundle in `dist/` that a release contains, by platform.
fn bundle_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "stardust.app"
    } else {
        "dist"
    }
}

/// Packages `dist/`, adds it to `out/manifest.json` and signs the manifest.
pub fn release(key_path: &Path, out: &Path) {
    let key = read_key(key_path);
    let version = env!("CARGO_PKG_VERSION");
    let platform = update::platform();
    fs::create_dir_all(out).expect("failed to create the release folder");

    let file_name = format!("stardust-{version}-{platform}.tar.gz");
    let package = package(Path::new("dist"));
    fs::write(out.join(&file_name), &package).expect("failed to write the package");

    let manifest_path = out.join("manifest.json");
    let mut manifest = fs::read(&manifest_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<update::Manifest>(&bytes).ok())
        .filter(|manifest| manifest.version == version)
        .unwrap_or_else(|| update::Manifest {
            version: version.to_owned(),
            packages: Default::default(),
        });
    let sha256: String = Sha256::digest(&package)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    manifest.packages.insert(
        platform.clone(),
        update::Package {
            url: file_name.clone(),
            size: package.len() as u64,
            sha256,
        },
    );
    let bytes = serde_json::to_vec_pretty(&manifest).expect("failed to write the manifest");
    fs::write(&manifest_path, &bytes).expect("failed to write the manifest");
    let signature = BASE64.encode(key.sign(&bytes).to_bytes());
    fs::write(out.join("manifest.json.sig"), format!("{signature}\n"))
        .expect("failed to write the signature");
    println!(
        "Released {version} for {platform}: {} and a signed manifest.json in {}",
        file_name,
        out.display()
    );
}

/// A gzipped tar of the bundle, with one top-level item named like the install folder.
fn package(dist: &Path) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    // Keep symbolic links as links: macOS frameworks rely on them.
    builder.follow_symlinks(false);
    let name = bundle_name();
    let source = if name == "dist" {
        dist.to_path_buf()
    } else {
        dist.join(name)
    };
    builder
        .append_dir_all(name, &source)
        .expect("failed to package the bundle");
    builder
        .into_inner()
        .and_then(|encoder| encoder.finish())
        .expect("failed to package the bundle")
}
