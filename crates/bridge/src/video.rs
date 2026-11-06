//! Hardware video decoding. macOS and Windows use it by default. On Linux, Chromium
//! needs VA-API turned on, which only helps where a VA-API driver is installed.

use std::path::PathBuf;

/// The Chromium features that decode video with VA-API on Linux. Milestone 0 measured
/// CPU on a 1080p video falling from 141% to 52% of a core with them.
pub const VAAPI_FEATURES: [&str; 2] = ["AcceleratedVideoDecodeLinuxGL", "VaapiVideoDecoder"];

/// Where VA-API drivers live: `LIBVA_DRIVERS_PATH` if it's set, or the usual folders.
pub fn vaapi_driver_folders(drivers_path: Option<&str>) -> Vec<PathBuf> {
    match drivers_path.filter(|path| !path.is_empty()) {
        Some(path) => path.split(':').map(PathBuf::from).collect(),
        None => [
            "/usr/lib/dri",
            "/usr/lib64/dri",
            "/usr/lib/x86_64-linux-gnu/dri",
            "/usr/lib/aarch64-linux-gnu/dri",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
    }
}

/// Whether any folder holds a VA-API driver, such as `iHD_drv_video.so`.
pub fn has_vaapi_driver(folders: &[PathBuf]) -> bool {
    folders.iter().any(|folder| {
        std::fs::read_dir(folder).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with("_drv_video.so")
            })
        })
    })
}

/// Adds features to an `--enable-features` value, keeping any already given.
pub fn add_features(existing: &str, added: &[&str]) -> String {
    let mut features: Vec<&str> = existing
        .split(',')
        .filter(|feature| !feature.is_empty())
        .collect();
    for feature in added {
        if !features.contains(feature) {
            features.push(feature);
        }
    }
    features.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_drivers_where_libva_looks() {
        assert_eq!(
            vaapi_driver_folders(Some("/a:/b")),
            [PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert!(vaapi_driver_folders(None).contains(&PathBuf::from("/usr/lib/dri")));
        assert_eq!(vaapi_driver_folders(Some("")), vaapi_driver_folders(None));
    }

    #[test]
    fn notices_an_installed_driver() {
        let folder = std::env::temp_dir().join(format!("stardust-vaapi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!has_vaapi_driver(&[folder.clone()]));
        std::fs::write(folder.join("iHD_drv_video.so"), b"").unwrap();
        assert!(has_vaapi_driver(&[
            PathBuf::from("/missing"),
            folder.clone()
        ]));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn keeps_features_already_asked_for() {
        assert_eq!(add_features("", &VAAPI_FEATURES), VAAPI_FEATURES.join(","));
        assert_eq!(
            add_features("Foo,VaapiVideoDecoder", &VAAPI_FEATURES),
            "Foo,VaapiVideoDecoder,AcceleratedVideoDecodeLinuxGL"
        );
    }
}
