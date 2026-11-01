use serde::{Deserialize, Serialize};

/// A tab's Back and Forward list, kept by the browser for restored tabs. CEF can't put
/// a saved list back into a new browser, so these tabs navigate from this list instead.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabHistory {
    pub entries: Vec<String>,
    pub index: usize,
    /// The address a Back or Forward is on its way to, so that arriving there isn't
    /// counted as a new visit.
    #[serde(skip)]
    pending: Option<String>,
}

/// Long lists are trimmed from the oldest end when saved.
const MAX_ENTRIES: usize = 50;

impl TabHistory {
    pub fn new(entries: Vec<String>, index: usize) -> Self {
        let mut history = Self {
            entries,
            index,
            pending: None,
        };
        if history.entries.len() > MAX_ENTRIES {
            let drop = history.entries.len() - MAX_ENTRIES;
            history.entries.drain(..drop);
            history.index = history.index.saturating_sub(drop);
        }
        history.index = history.index.min(history.entries.len().saturating_sub(1));
        history
    }

    pub fn can_go_back(&self) -> bool {
        self.index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    /// Moves back and returns the address to load.
    pub fn go_back(&mut self) -> Option<String> {
        self.can_go_back().then(|| self.move_to(self.index - 1))
    }

    /// Moves forward and returns the address to load.
    pub fn go_forward(&mut self) -> Option<String> {
        self.can_go_forward().then(|| self.move_to(self.index + 1))
    }

    fn move_to(&mut self, index: usize) -> String {
        self.index = index;
        let url = self.entries[index].clone();
        self.pending = Some(url.clone());
        url
    }

    /// Records that the tab arrived at an address.
    pub fn arrived(&mut self, url: &str) {
        if self.pending.take().is_some() {
            // A Back or Forward landed, perhaps redirected. Keep the final address.
            if let Some(entry) = self.entries.get_mut(self.index) {
                *entry = url.to_owned();
            }
            return;
        }
        if self
            .entries
            .get(self.index)
            .is_some_and(|entry| entry == url)
        {
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(url.to_owned());
        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        self.index = self.entries.len() - 1;
    }
}

/// File types that can run code or change the system when opened. Opening one of these
/// asks first. Compared case-insensitively against the last extension.
const RISKY_EXTENSIONS: [&str; 34] = [
    "exe", "msi", "bat", "cmd", "com", "scr", "pif", "ps1", "vbs", "vbe", "js", "jse", "wsf",
    "wsh", "hta", "lnk", "reg", "jar", "sh", "bash", "zsh", "csh", "py", "pl", "rb", "run", "bin",
    "appimage", "desktop", "deb", "rpm", "dmg", "pkg", "command",
];

pub fn is_risky_file(file_name: &str) -> bool {
    file_name.rsplit_once('.').is_some_and(|(_, extension)| {
        RISKY_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> TabHistory {
        TabHistory::new(vec!["a".into(), "b".into(), "c".into()], 1)
    }

    #[test]
    fn moves_through_a_saved_list() {
        let mut history = history();
        assert_eq!(history.go_back().as_deref(), Some("a"));
        history.arrived("a");
        assert!(!history.can_go_back());
        assert_eq!(history.go_back(), None);
        assert_eq!(history.go_forward().as_deref(), Some("b"));
        history.arrived("b");
        assert_eq!(history.go_forward().as_deref(), Some("c"));
        history.arrived("c");
        assert!(!history.can_go_forward());
    }

    #[test]
    fn a_new_visit_replaces_the_forward_list() {
        let mut history = history();
        history.arrived("new");
        assert_eq!(history.entries, ["a", "b", "new"]);
        assert!(!history.can_go_forward());
        assert!(history.can_go_back());
    }

    #[test]
    fn arriving_at_the_current_address_changes_nothing() {
        let mut history = history();
        history.arrived("b");
        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.index, 1);
    }

    #[test]
    fn a_redirect_during_back_keeps_the_final_address() {
        let mut history = history();
        history.go_back();
        history.arrived("a-redirected");
        assert_eq!(history.entries, ["a-redirected", "b", "c"]);
        assert_eq!(history.index, 0);
    }

    #[test]
    fn saved_lists_are_clamped_and_trimmed() {
        assert_eq!(TabHistory::new(vec!["a".into()], 9).index, 0);
        let long: Vec<String> = (0..60).map(|n| n.to_string()).collect();
        let trimmed = TabHistory::new(long, 59);
        assert_eq!(trimmed.entries.len(), MAX_ENTRIES);
        assert_eq!(trimmed.entries[trimmed.index], "59");
        assert_eq!(TabHistory::new(Vec::new(), 3).index, 0);
    }

    #[test]
    fn an_empty_list_has_nowhere_to_go() {
        let mut history = TabHistory::default();
        assert_eq!(history.go_back(), None);
        assert_eq!(history.go_forward(), None);
    }

    #[test]
    fn risky_files_are_recognized() {
        for name in [
            "setup.EXE",
            "run.sh",
            "app.AppImage",
            "x.desktop",
            "a.tar.bat",
        ] {
            assert!(is_risky_file(name), "{name}");
        }
        for name in [
            "photo.png",
            "notes.txt",
            "report.pdf",
            "archive.tar.gz",
            "noextension",
        ] {
            assert!(!is_risky_file(name), "{name}");
        }
    }
}
