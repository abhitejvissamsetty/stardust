//! Browser logic that doesn't depend on CEF: the messages between the toolbar and
//! Rust, address bar parsing, keyboard shortcuts and the error page.

mod address;
mod downloads;
mod error_page;
mod history;
mod messages;
mod session;
mod settings;
mod shortcuts;
pub mod storage_types;
mod tabs;

pub use address::resolve_input;
pub use downloads::{Download, DownloadList, DownloadState, safe_file_name, unique_path};
pub use error_page::error_page;
pub use history::{TabHistory, is_risky_file};
pub use messages::{BrowserState, UiCommand, UiEvent};
pub use session::{SavedWindows, Session, SessionTab};
pub use settings::{
    DEFAULT_HOMEPAGE, InternalPage, PageCommand, SearchEngine, Settings, StartupMode, is_web_url,
    normalize_homepage,
};
pub use shortcuts::{KeyPress, Shortcut, shortcut_for};
pub use tabs::{Tab, TabId, TabStrip};
