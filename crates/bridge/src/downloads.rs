use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    InProgress,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub id: u32,
    pub file_name: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub state: DownloadState,
    pub received_bytes: i64,
    /// Negative or zero when the server didn't say.
    pub total_bytes: i64,
    pub bytes_per_second: i64,
}

/// The downloads shown in the download bar, newest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DownloadList {
    items: Vec<Download>,
}

impl DownloadList {
    pub fn items(&self) -> &[Download] {
        &self.items
    }

    pub fn get(&self, id: u32) -> Option<&Download> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn add(&mut self, download: Download) {
        self.items.insert(0, download);
    }

    /// Applies a progress report. Reports for unknown downloads are ignored.
    pub fn update(&mut self, id: u32, state: DownloadState, received: i64, total: i64, speed: i64) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.state = state;
            item.received_bytes = received;
            item.total_bytes = total;
            item.bytes_per_second = speed;
        }
    }

    /// Removes a finished download from the list. The file stays on disk.
    pub fn dismiss(&mut self, id: u32) {
        self.items
            .retain(|item| item.id != id || item.state == DownloadState::InProgress);
    }

    /// Removes every finished download from the list. The files stay on disk.
    pub fn clear_finished(&mut self) {
        self.items
            .retain(|item| item.state == DownloadState::InProgress);
    }

    /// Whether a current download already claims this path.
    pub fn claims(&self, path: &Path) -> bool {
        self.items
            .iter()
            .any(|item| item.state == DownloadState::InProgress && item.path == path)
    }
}

/// Turns a name suggested by a website into a safe file name. Sites control this name,
/// so it must not be able to leave the downloads folder or hide as a dotfile.
pub fn safe_file_name(suggested: &str) -> String {
    let base = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned
        .trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        return "download".to_owned();
    }

    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let is_windows_device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit()));
    if is_windows_device {
        format!("_{cleaned}")
    } else {
        cleaned.to_owned()
    }
}

/// Finds a path in `folder` that isn't taken, numbering the name if it needs to:
/// `report.tar.gz` becomes `report (1).tar.gz`.
pub fn unique_path(folder: &Path, file_name: &str, is_taken: impl Fn(&Path) -> bool) -> PathBuf {
    let path = folder.join(file_name);
    if !is_taken(&path) {
        return path;
    }
    let (stem, extension) = match file_name.split_once('.') {
        Some((stem, extension)) => (stem, format!(".{extension}")),
        None => (file_name, String::new()),
    };
    (1u32..)
        .map(|number| folder.join(format!("{stem} ({number}){extension}")))
        .find(|path| !is_taken(path))
        .expect("Ran out of file names")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn download(id: u32, state: DownloadState) -> Download {
        Download {
            id,
            file_name: format!("{id}.bin"),
            path: PathBuf::from(format!("/dl/{id}.bin")),
            state,
            received_bytes: 0,
            total_bytes: 0,
            bytes_per_second: 0,
        }
    }

    #[test]
    fn site_names_cannot_leave_the_folder() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("..\\..\\evil.exe"), "evil.exe");
        assert_eq!(safe_file_name("/abs/path.txt"), "path.txt");
        assert_eq!(safe_file_name(".."), "download");
    }

    #[test]
    fn site_names_are_cleaned() {
        assert_eq!(safe_file_name(".bashrc"), "bashrc");
        assert_eq!(safe_file_name("a:b*c?.txt"), "a_b_c_.txt");
        assert_eq!(safe_file_name("line\nbreak.txt"), "line_break.txt");
        assert_eq!(safe_file_name("name. "), "name");
        assert_eq!(safe_file_name(""), "download");
        assert_eq!(safe_file_name("résumé 2026.pdf"), "résumé 2026.pdf");
    }

    #[test]
    fn windows_device_names_are_avoided() {
        assert_eq!(safe_file_name("NUL.txt"), "_NUL.txt");
        assert_eq!(safe_file_name("com1"), "_com1");
        assert_eq!(safe_file_name("company.txt"), "company.txt");
    }

    #[test]
    fn unique_paths_never_overwrite() {
        let folder = Path::new("/dl");
        assert_eq!(
            unique_path(folder, "a.txt", |_| false),
            Path::new("/dl/a.txt")
        );
        let taken = |path: &Path| {
            path == Path::new("/dl/a.tar.gz") || path == Path::new("/dl/a (1).tar.gz")
        };
        assert_eq!(
            unique_path(folder, "a.tar.gz", taken),
            Path::new("/dl/a (2).tar.gz")
        );
        let taken = |path: &Path| path == Path::new("/dl/noext");
        assert_eq!(
            unique_path(folder, "noext", taken),
            Path::new("/dl/noext (1)")
        );
    }

    #[test]
    fn newest_downloads_come_first() {
        let mut list = DownloadList::default();
        list.add(download(1, DownloadState::Complete));
        list.add(download(2, DownloadState::InProgress));
        assert_eq!(list.items()[0].id, 2);
    }

    #[test]
    fn updates_apply_to_known_downloads_only() {
        let mut list = DownloadList::default();
        list.add(download(1, DownloadState::InProgress));
        list.update(1, DownloadState::Complete, 10, 10, 0);
        list.update(99, DownloadState::Failed, 0, 0, 0);
        assert_eq!(list.items().len(), 1);
        assert_eq!(list.get(1).unwrap().state, DownloadState::Complete);
        assert_eq!(list.get(1).unwrap().received_bytes, 10);
    }

    #[test]
    fn active_downloads_cannot_be_dismissed() {
        let mut list = DownloadList::default();
        list.add(download(1, DownloadState::InProgress));
        list.add(download(2, DownloadState::Complete));
        list.add(download(3, DownloadState::Failed));
        list.dismiss(1);
        list.dismiss(2);
        assert_eq!(list.items().len(), 2);
        list.clear_finished();
        assert_eq!(list.items().len(), 1);
        assert_eq!(list.items()[0].id, 1);
    }

    #[test]
    fn only_active_downloads_claim_a_path() {
        let mut list = DownloadList::default();
        list.add(download(1, DownloadState::InProgress));
        list.add(download(2, DownloadState::Cancelled));
        assert!(list.claims(Path::new("/dl/1.bin")));
        assert!(!list.claims(Path::new("/dl/2.bin")));
    }

    #[test]
    fn the_toolbar_never_receives_the_full_path() {
        let json = serde_json::to_string(&download(1, DownloadState::InProgress)).unwrap();
        assert!(!json.contains("/dl/"));
        assert!(json.contains(r#""state":"inProgress""#));
    }
}
