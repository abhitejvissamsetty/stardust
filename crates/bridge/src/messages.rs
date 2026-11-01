use serde::{Deserialize, Serialize};

use crate::{Download, InternalPage, TabId, TabStrip};

/// Sent by the toolbar page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UiCommand {
    Ready,
    Navigate {
        input: String,
    },
    Back,
    Forward,
    Reload,
    Stop,
    NewTab,
    SelectTab {
        id: TabId,
    },
    CloseTab {
        id: TabId,
    },
    MoveTab {
        id: TabId,
        index: usize,
    },
    CancelDownload {
        id: u32,
    },
    DismissDownload {
        id: u32,
    },
    ShowDownload {
        id: u32,
    },
    ClearDownloads,
    ToggleBookmark,
    Find {
        text: String,
        forward: bool,
        again: bool,
    },
    CloseFind,
    OpenPage {
        page: InternalPage,
    },
}

/// Sent to the toolbar page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum UiEvent<'a> {
    Tabs {
        strip: &'a TabStrip,
    },
    Downloads {
        items: &'a [Download],
    },
    FocusAddress,
    ShowFind,
    HideFind,
    FindResult {
        count: i32,
        active: i32,
    },
    /// Focuses the address bar without disturbing text already being typed.
    KeepAddressFocus,
}

/// Everything the toolbar shows about the current page.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub url: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub progress: f64,
    pub is_bookmarked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_from_the_toolbar() {
        let command: UiCommand =
            serde_json::from_str(r#"{"type":"navigate","input":"example.com"}"#).unwrap();
        assert_eq!(
            command,
            UiCommand::Navigate {
                input: "example.com".into()
            }
        );
        assert_eq!(
            serde_json::from_str::<UiCommand>(r#"{"type":"back"}"#).unwrap(),
            UiCommand::Back
        );
    }

    #[test]
    fn rejects_unknown_commands() {
        assert!(serde_json::from_str::<UiCommand>(r#"{"type":"formatDisk"}"#).is_err());
    }

    #[test]
    fn parses_tab_commands() {
        assert_eq!(
            serde_json::from_str::<UiCommand>(r#"{"type":"moveTab","id":2,"index":0}"#).unwrap(),
            UiCommand::MoveTab { id: 2, index: 0 }
        );
        assert_eq!(
            serde_json::from_str::<UiCommand>(r#"{"type":"newTab"}"#).unwrap(),
            UiCommand::NewTab
        );
    }

    #[test]
    fn parses_find_commands() {
        assert_eq!(
            serde_json::from_str::<UiCommand>(
                r#"{"type":"find","text":"a","forward":false,"again":true}"#
            )
            .unwrap(),
            UiCommand::Find {
                text: "a".into(),
                forward: false,
                again: true
            }
        );
    }

    #[test]
    fn events_use_javascript_field_names() {
        let strip = TabStrip::default();
        let json = serde_json::to_string(&UiEvent::Tabs { strip: &strip }).unwrap();
        assert!(json.contains(r#""type":"tabs""#));
        assert!(json.contains(r#""activeId":null"#));
    }
}
