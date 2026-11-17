//! Browser logic that doesn't depend on CEF: the messages between the toolbar and
//! Rust, address bar parsing, keyboard shortcuts and the error page.

mod address;
mod discard;
mod downloads;
mod error_page;
mod history;
mod logins;
mod messages;
mod permissions;
mod session;
mod settings;
mod shortcuts;
mod sidepanel;
pub mod storage_types;
mod tabs;
mod video;
mod workspaces;
mod zoom;

pub use address::resolve_input;
pub use discard::{
    DISCARD_CHECK_MS, DISCARD_IDLE_MS, DISCARD_LOADED_LIMIT, LoadedTab, tabs_to_discard,
};
pub use downloads::{Download, DownloadList, DownloadState, safe_file_name, unique_path};
pub use error_page::error_page;
pub use history::{TabHistory, is_risky_file};
pub use logins::{LOGIN_WATCHER, SubmittedLogin, fill_script, login_origin};
pub use messages::{BrowserState, UiCommand, UiEvent, WindowAction};
pub use permissions::{Answer, Decision, Permission, SitePermissions, describe, site_origin};
pub use session::{SavedWindows, Session, SessionTab};
pub use settings::{
    DEFAULT_HOMEPAGE, ExtensionSource, InternalPage, PageCommand, SearchEngine, Settings,
    StartupMode, is_web_url, normalize_homepage,
};
pub use shortcuts::{KeyPress, Shortcut, shortcut_for};
pub use sidepanel::{CLIP_SCRIPT, Clip, SidePanelData, Snippet, Todo};
pub use tabs::{Tab, TabId, TabStrip};
pub use video::{VAAPI_FEATURES, add_features, has_vaapi_driver, vaapi_driver_folders};
pub use workspaces::{
    COLORS as WORKSPACE_COLORS, MAIN_COLOR, MAIN_NAME, MAIN_WORKSPACE, Workspace, Workspaces,
    is_valid_id as is_workspace_id,
};
pub use zoom::{ZOOM_STEPS, level_for_percent, next_zoom_percent, percent_for_level};
