use serde::{Deserialize, Serialize};

use crate::{Download, InternalPage, TabId, TabStrip, storage_types};

/// Sent by the toolbar page.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
        #[serde(default)]
        match_case: bool,
    },
    CloseFind,
    OpenDownload {
        id: u32,
        confirmed: bool,
    },
    /// Asks for bookmarks and history matching what's typed in the address bar.
    Suggest {
        query: String,
    },
    /// Shows the menu under the toolbar with the given contents.
    ShowPopup {
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        contents: serde_json::Value,
    },
    HidePopup,
    PopupReady,
    /// Goes ahead with closing despite active downloads, or decides against it.
    ConfirmClose {
        close: bool,
    },
    AnswerDialog {
        accepted: bool,
        input: String,
    },
    /// A tab was dragged out of its window and dropped on nothing.
    TearOffTab {
        id: TabId,
    },
    /// A tab from another window was dropped on this window's tab strip.
    AdoptTab {
        from_window: u32,
        id: TabId,
    },
    OpenPage {
        page: InternalPage,
    },
    /// Fills the page's most recently used saved login.
    FillLogin,
    WindowAction {
        action: WindowAction,
    },
    /// The vertical tab list's page has loaded and can be sent the tabs.
    SidebarReady,
    /// Moves the tabs to the left side, or back to the top.
    SetVerticalTabs {
        enabled: bool,
    },
    /// Shrinks the vertical tab list to icons, or widens it again.
    SetSidebarCollapsed {
        collapsed: bool,
    },
    /// Shows a second tab beside this one, or goes back to one page.
    ToggleSplitView,
    NewWindow,
    NewPrivateWindow,
    /// Shows a workspace's window, or opens it with the tabs it had.
    OpenWorkspace {
        id: String,
    },
    OpenFind,
    Quit,
    ReopenClosedTab,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Print,
    DevTools,
    ToggleMuteTab {
        id: TabId,
    },
    /// Puts an extension's popup away, if one is showing.
    HideExtensionPopup,
    /// Shows an extension's popup with its corner at the given place.
    OpenExtensionPopup {
        id: String,
        left: i32,
        top: i32,
    },
    SetExtensionPinned {
        id: String,
        pinned: bool,
    },
    /// Turns an extension off for the next start.
    SetExtensionEnabled {
        id: String,
        enabled: bool,
    },
    /// Asks how the window is showing, which the toolbar learns through `WindowState`.
    GetWindowState,
}

impl UiCommand {
    /// What the vertical tab list's page may ask for. It's a second page the browser
    /// trusts, so it gets only what a tab list needs, not the whole set.
    pub fn allowed_from_sidebar(&self) -> bool {
        matches!(
            self,
            Self::SidebarReady
                | Self::NewTab
                | Self::SelectTab { .. }
                | Self::CloseTab { .. }
                | Self::MoveTab { .. }
                | Self::ToggleMuteTab { .. }
                | Self::ToggleSplitView
                | Self::SetVerticalTabs { .. }
                | Self::SetSidebarCollapsed { .. }
                | Self::OpenWorkspace { .. }
        )
    }
}

/// What the person asked of the window, as Chrome's title bar and menus do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowAction {
    Minimize,
    /// Maximizes, or restores a window that already is.
    ToggleMaximize,
    ToggleFullscreen,
}

/// Sent to the toolbar page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UiEvent<'a> {
    Tabs {
        strip: &'a TabStrip,
        window_id: u32,
        /// A private window, which looks different and keeps nothing.
        private: bool,
        /// The two tabs shown side by side, left then right, if there are two.
        split: Option<(TabId, TabId)>,
    },
    /// A page's icon, as a `data:` address, sent once for the toolbar to remember.
    Favicon {
        url: &'a str,
        data: &'a str,
    },
    /// The extensions that are on, for the toolbar's extensions menu and pinned buttons.
    Extensions {
        items: &'a [serde_json::Value],
        /// Private windows don't run extensions.
        available: bool,
    },
    /// Where the tabs are shown, for the toolbar and the vertical tab list.
    Layout {
        vertical: bool,
        collapsed: bool,
    },
    /// The workspaces to switch between, and the one this window belongs to.
    Workspaces {
        items: &'a [serde_json::Value],
        current: &'a str,
    },
    Downloads {
        items: &'a [Download],
    },
    FocusAddress,
    Suggestions {
        query: &'a str,
        items: &'a [storage_types::SuggestionItem],
    },
    BookmarksBar {
        bookmarks: &'a [storage_types::SuggestionItem],
        folders: &'a [storage_types::FolderItems],
    },
    /// Closing would cancel this many downloads.
    ConfirmClose {
        downloads: usize,
    },
    /// A page asks something. `kind` is alert, confirm, prompt or leave.
    ShowDialog {
        kind: &'a str,
        origin: &'a str,
        message: &'a str,
        default_text: &'a str,
    },
    HideDialog,
    /// The page took focus, so the address bar stops editing.
    PageFocused,
    ShowFind,
    HideFind,
    FindResult {
        count: i32,
        active: i32,
    },
    /// Focuses the address bar without disturbing text already being typed.
    KeepAddressFocus,
    /// The menu has opened, and the toolbar has focus back.
    MenuReady,
    WindowState {
        maximized: bool,
        minimized: bool,
        fullscreen: bool,
    },
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
    /// The page's site has saved logins, so the toolbar offers to fill one.
    pub has_logins: bool,
    /// How far the page is zoomed. Nothing sent yet means 100.
    pub zoom_percent: i32,
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
    fn the_sidebar_may_only_ask_for_tab_things() {
        let allowed = |json: &str| {
            serde_json::from_str::<UiCommand>(json)
                .unwrap()
                .allowed_from_sidebar()
        };
        assert!(allowed(r#"{"type":"selectTab","id":1}"#));
        assert!(allowed(r#"{"type":"closeTab","id":1}"#));
        assert!(allowed(r#"{"type":"moveTab","id":1,"index":0}"#));
        assert!(allowed(r#"{"type":"newTab"}"#));
        assert!(!allowed(r#"{"type":"navigate","input":"example.com"}"#));
        assert!(!allowed(
            r#"{"type":"openDownload","id":0,"confirmed":true}"#
        ));
        assert!(!allowed(r#"{"type":"openPage","page":"settings"}"#));
        assert!(!allowed(r#"{"type":"quit"}"#));
        assert!(!allowed(r#"{"type":"ready"}"#));
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
                again: true,
                match_case: false,
            }
        );
    }

    #[test]
    fn events_use_javascript_field_names() {
        let strip = TabStrip::default();
        let json = serde_json::to_string(&UiEvent::Tabs {
            strip: &strip,
            window_id: 1,
            private: false,
            split: None,
        })
        .unwrap();
        assert!(json.contains(r#""type":"tabs""#));
        assert!(json.contains(r#""activeId":null"#));
    }
}
