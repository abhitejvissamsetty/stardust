use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchEngine {
    #[default]
    DuckDuckGo,
    Google,
    Bing,
    Brave,
}

impl SearchEngine {
    pub fn search_url(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "https://duckduckgo.com/?q=",
            Self::Google => "https://www.google.com/search?q=",
            Self::Bing => "https://www.bing.com/search?q=",
            Self::Brave => "https://search.brave.com/search?q=",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckDuckGo",
            Self::Google => "google",
            Self::Bing => "bing",
            Self::Brave => "brave",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::DuckDuckGo, Self::Google, Self::Bing, Self::Brave]
            .into_iter()
            .find(|engine| engine.id() == id)
    }
}

/// What the browser opens when it starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupMode {
    #[default]
    RestoreTabs,
    Homepage,
}

impl StartupMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::RestoreTabs => "restoreTabs",
            Self::Homepage => "homepage",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [Self::RestoreTabs, Self::Homepage]
            .into_iter()
            .find(|mode| mode.id() == id)
    }
}

pub const DEFAULT_HOMEPAGE: &str = "https://duckduckgo.com/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub search_engine: SearchEngine,
    pub homepage: String,
    pub startup: StartupMode,
    pub ask_download_location: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search_engine: SearchEngine::default(),
            homepage: DEFAULT_HOMEPAGE.to_owned(),
            startup: StartupMode::default(),
            ask_download_location: false,
        }
    }
}

/// Checks a typed homepage. Only web addresses are allowed, and a bare domain gets `https://`.
pub fn normalize_homepage(input: &str) -> Option<String> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let url = if lower.starts_with("http://") || lower.starts_with("https://") {
        input.to_owned()
    } else {
        // Reuses address bar parsing, where a search result means it wasn't an address.
        let resolved = crate::resolve_input(input, "search:")?;
        if resolved.starts_with("search:") || !resolved.starts_with("http") {
            return None;
        }
        resolved
    };
    (!url.contains(char::is_whitespace)).then_some(url)
}

/// A browser page that has access to saved data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InternalPage {
    History,
    Bookmarks,
    Settings,
    Downloads,
}

impl InternalPage {
    pub const ALL: [Self; 4] = [
        Self::History,
        Self::Bookmarks,
        Self::Settings,
        Self::Downloads,
    ];

    pub fn address(self) -> &'static str {
        match self {
            Self::History => "browser://history",
            Self::Bookmarks => "browser://bookmarks",
            Self::Settings => "browser://settings",
            Self::Downloads => "browser://downloads",
        }
    }

    pub fn from_address(input: &str) -> Option<Self> {
        let input = input.trim().trim_end_matches('/').to_ascii_lowercase();
        Self::ALL.into_iter().find(|page| page.address() == input)
    }
}

/// Sent by an internal page. Only those pages can send these, and they can't choose
/// what happens to the rest of the browser.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PageCommand {
    ListHistory {
        query: String,
    },
    DeleteHistory {
        url: String,
    },
    ClearHistory,
    ListBookmarks {
        query: String,
    },
    RemoveBookmark {
        url: String,
    },
    GetSettings,
    SetSearchEngine {
        engine: SearchEngine,
    },
    SetHomepage {
        homepage: String,
    },
    SetStartup {
        startup: StartupMode,
    },
    SetAskDownloadLocation {
        ask: bool,
    },
    UpdateBookmark {
        url: String,
        new_url: String,
        title: String,
        folder_id: Option<i64>,
    },
    CreateFolder {
        name: String,
    },
    RenameFolder {
        id: i64,
        name: String,
    },
    DeleteFolder {
        id: i64,
    },
    ListDownloads,
    OpenSavedDownload {
        path: String,
        confirmed: bool,
    },
    ShowSavedDownload {
        path: String,
    },
    RemoveSavedDownload {
        path: String,
    },
    ClearSavedDownloads,
    ClearData {
        history: bool,
        cookies: bool,
        cache: bool,
        downloads: bool,
    },
    ListImportSources,
    Import {
        source: String,
    },
}

/// Whether a page's URL can be bookmarked or kept in history.
pub fn is_web_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homepages_must_be_web_addresses() {
        assert_eq!(
            normalize_homepage("https://a.com/x").as_deref(),
            Some("https://a.com/x")
        );
        assert_eq!(
            normalize_homepage(" example.com ").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            normalize_homepage("localhost:3000").as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(normalize_homepage("javascript:alert(1)"), None);
        assert_eq!(normalize_homepage("file:///etc/passwd"), None);
        assert_eq!(normalize_homepage("not an address"), None);
        assert_eq!(normalize_homepage("https://a.com/a b"), None);
        assert_eq!(normalize_homepage(""), None);
    }

    #[test]
    fn search_engines_round_trip_by_id() {
        for engine in [
            SearchEngine::DuckDuckGo,
            SearchEngine::Google,
            SearchEngine::Bing,
            SearchEngine::Brave,
        ] {
            assert_eq!(SearchEngine::from_id(engine.id()), Some(engine));
            let json = serde_json::to_string(&engine).unwrap();
            assert_eq!(json, format!("\"{}\"", engine.id()));
            assert!(engine.search_url().starts_with("https://"));
        }
        assert_eq!(SearchEngine::from_id("evil"), None);
    }

    #[test]
    fn startup_modes_round_trip_by_id() {
        for mode in [StartupMode::RestoreTabs, StartupMode::Homepage] {
            assert_eq!(StartupMode::from_id(mode.id()), Some(mode));
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                format!("\"{}\"", mode.id())
            );
        }
        assert_eq!(StartupMode::from_id("x"), None);
    }

    #[test]
    fn internal_pages_are_found_by_address() {
        assert_eq!(
            InternalPage::from_address("browser://history"),
            Some(InternalPage::History)
        );
        assert_eq!(
            InternalPage::from_address(" BROWSER://Settings/ "),
            Some(InternalPage::Settings)
        );
        assert_eq!(InternalPage::from_address("browser://other"), None);
        assert_eq!(InternalPage::from_address("history"), None);
    }

    #[test]
    fn page_commands_reject_unknown_values() {
        assert!(
            serde_json::from_str::<PageCommand>(r#"{"type":"setSearchEngine","engine":"evil"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<PageCommand>(r#"{"type":"newTab"}"#).is_err());
        assert_eq!(
            serde_json::from_str::<PageCommand>(r#"{"type":"clearHistory"}"#).unwrap(),
            PageCommand::ClearHistory
        );
    }

    #[test]
    fn page_commands_use_javascript_field_names() {
        let json = r#"{"type":"updateBookmark","url":"a","newUrl":"b","title":"t","folderId":3}"#;
        assert_eq!(
            serde_json::from_str::<PageCommand>(json).unwrap(),
            PageCommand::UpdateBookmark {
                url: "a".into(),
                new_url: "b".into(),
                title: "t".into(),
                folder_id: Some(3)
            }
        );
    }

    #[test]
    fn only_web_pages_are_kept() {
        assert!(is_web_url("https://a.com"));
        assert!(is_web_url("http://a.com"));
        assert!(!is_web_url("file:///a"));
        assert!(!is_web_url("about:blank"));
        assert!(!is_web_url("browser://history"));
        assert!(!is_web_url("data:text/html,x"));
    }
}
