use serde::{Deserialize, Serialize};

use crate::{InternalPage, TabStrip, is_web_url};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
}

/// The open tabs, saved so they can come back after a restart or a crash.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
}

fn can_restore(url: &str) -> bool {
    is_web_url(url) || InternalPage::from_address(url).is_some()
}

impl Session {
    /// Blank tabs and anything else that can't be reopened are left out.
    pub fn from_strip(strip: &TabStrip) -> Self {
        let mut session = Self::default();
        for tab in strip.tabs() {
            if can_restore(&tab.page.url) {
                if Some(tab.id) == strip.active_id() {
                    session.active = session.tabs.len();
                }
                session.tabs.push(SessionTab {
                    url: tab.page.url.clone(),
                    title: tab.title.clone(),
                });
            }
        }
        session
    }

    /// Saved URLs are checked again, because the file could have been edited.
    fn sanitized(mut self) -> Self {
        self.tabs.retain(|tab| can_restore(&tab.url));
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self
    }
}

/// Every open window's tabs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWindows {
    pub windows: Vec<Session>,
}

impl SavedWindows {
    /// Reads saved windows, or a session saved by a version with one window. Damaged
    /// data gives no windows rather than an error.
    pub fn parse(json: &str) -> Self {
        let windows = serde_json::from_str::<Self>(json)
            .map(|saved| saved.windows)
            .or_else(|_| serde_json::from_str::<Session>(json).map(|session| vec![session]))
            .unwrap_or_default();
        Self {
            windows: windows
                .into_iter()
                .map(Session::sanitized)
                .filter(|session| !session.tabs.is_empty())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_with(urls: &[&str], active: usize) -> TabStrip {
        let mut strip = TabStrip::default();
        let ids: Vec<_> = urls
            .iter()
            .map(|url| {
                let id = strip.add();
                strip.select(id);
                strip.get_mut(id).unwrap().page.url = (*url).to_owned();
                strip.get_mut(id).unwrap().title = format!("title of {url}");
                id
            })
            .collect();
        strip.select(ids[active]);
        strip
    }

    #[test]
    fn saves_tabs_in_order_with_the_active_one() {
        let session = Session::from_strip(&strip_with(&["https://a.com", "https://b.com"], 1));
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[0].url, "https://a.com");
        assert_eq!(session.tabs[1].title, "title of https://b.com");
        assert_eq!(session.active, 1);
    }

    #[test]
    fn leaves_out_tabs_that_cannot_be_reopened() {
        let urls = [
            "about:blank",
            "https://a.com",
            "",
            "browser://history",
            "file:///x",
        ];
        let session = Session::from_strip(&strip_with(&urls, 3));
        let saved: Vec<_> = session.tabs.iter().map(|tab| tab.url.as_str()).collect();
        assert_eq!(saved, ["https://a.com", "browser://history"]);
        assert_eq!(session.active, 1);
    }

    #[test]
    fn a_blank_active_tab_falls_back_to_the_first() {
        let session = Session::from_strip(&strip_with(&["https://a.com", "about:blank"], 1));
        assert_eq!(session.active, 0);
    }

    #[test]
    fn windows_round_trip_through_json() {
        let saved = SavedWindows {
            windows: vec![
                Session::from_strip(&strip_with(&["https://a.com", "https://b.com"], 1)),
                Session::from_strip(&strip_with(&["https://c.com"], 0)),
            ],
        };
        let json = serde_json::to_string(&saved).unwrap();
        assert_eq!(SavedWindows::parse(&json), saved);
    }

    #[test]
    fn reads_sessions_saved_with_one_window() {
        let session = Session::from_strip(&strip_with(&["https://a.com"], 0));
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(SavedWindows::parse(&json).windows, vec![session]);
    }

    #[test]
    fn damaged_sessions_are_empty() {
        assert!(SavedWindows::parse("not json").windows.is_empty());
        assert!(SavedWindows::parse("").windows.is_empty());
    }

    #[test]
    fn windows_with_nothing_to_restore_are_dropped() {
        let json = r#"{"windows":[{"tabs":[],"active":0},{"tabs":[{"url":"javascript:x","title":""}],"active":0}]}"#;
        assert!(SavedWindows::parse(json).windows.is_empty());
    }

    #[test]
    fn edited_sessions_cannot_restore_unsafe_urls() {
        let json = r#"{"windows":[{"tabs":[{"url":"javascript:alert(1)","title":""},{"url":"https://a.com","title":""}],"active":9}]}"#;
        let saved = SavedWindows::parse(json);
        assert_eq!(saved.windows[0].tabs.len(), 1);
        assert_eq!(saved.windows[0].tabs[0].url, "https://a.com");
        assert_eq!(saved.windows[0].active, 0);
    }
}
