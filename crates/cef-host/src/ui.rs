//! Builds the browser's own pages into `data:` URLs.

use std::collections::HashMap;
use std::sync::OnceLock;

use bridge::InternalPage;
use cef::*;

const INDEX: &str = include_str!("../../../ui/index.html");
const TOKENS: &str = include_str!("../../../ui/tokens.css");
const TOOLBAR: &str = include_str!("../../../ui/toolbar.css");
const APP: &str = include_str!("../../../ui/app.js");
const HISTORY: &str = include_str!("../../../ui/history.html");
const BOOKMARKS: &str = include_str!("../../../ui/bookmarks.html");
const SETTINGS: &str = include_str!("../../../ui/settings.html");
const DOWNLOADS: &str = include_str!("../../../ui/downloads.html");
const EXTENSIONS: &str = include_str!("../../../ui/extensions.html");
const SIDEBAR: &str = include_str!("../../../ui/sidebar.html");
const POPUP: &str = include_str!("../../../ui/popup.html");
const POPUP_JS: &str = include_str!("../../../ui/popup.js");
const PAGES_CSS: &str = include_str!("../../../ui/pages.css");
const PAGES_JS: &str = include_str!("../../../ui/pages.js");

/// The window name the toolbar gives the menu it opens.
pub const MENU_FRAME_NAME: &str = "stardust-menu";

/// The toolbar page, which carries the menu's page so it can open the menu itself.
/// Opened that way, the menu shares the toolbar's renderer process instead of
/// starting one of its own.
pub fn toolbar_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(build_toolbar_url)
}

fn build_toolbar_url() -> String {
    let menu = serde_json::to_string(&menu_html())
        .expect("Failed to encode the menu")
        // The menu has its own `</script>`, which would end the toolbar's script early.
        .replace("</", "<\\/");
    data_url(
        &INDEX
            .replace("/*TOKENS*/", TOKENS)
            .replace("/*TOOLBAR*/", TOOLBAR)
            .replace("/*APP*/", &APP.replace("/*MENU*/\"\"", &menu)),
    )
}

/// The vertical tab list's page.
pub fn sidebar_url() -> &'static str {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| data_url(&SIDEBAR.replace("/*TOKENS*/", TOKENS)))
}

fn menu_html() -> String {
    POPUP
        .replace("/*TOKENS*/", TOKENS)
        .replace("/*SCRIPT*/", POPUP_JS)
}

fn data_url(html: &str) -> String {
    let encoded = CefString::from(&base64_encode(Some(html.as_bytes())));
    format!("data:text/html;base64,{encoded}")
}

fn internal_urls() -> &'static HashMap<InternalPage, String> {
    static URLS: OnceLock<HashMap<InternalPage, String>> = OnceLock::new();
    URLS.get_or_init(|| {
        InternalPage::ALL
            .into_iter()
            .map(|page| {
                let html = match page {
                    InternalPage::History => HISTORY,
                    InternalPage::Bookmarks => BOOKMARKS,
                    InternalPage::Settings => SETTINGS,
                    InternalPage::Downloads => DOWNLOADS,
                    InternalPage::Extensions => EXTENSIONS,
                };
                let html = html
                    .replace("/*TOKENS*/", TOKENS)
                    .replace("/*PAGES*/", PAGES_CSS)
                    .replace("/*SCRIPT*/", PAGES_JS);
                (page, data_url(&html))
            })
            .collect()
    })
}

pub fn internal_url(page: InternalPage) -> &'static str {
    &internal_urls()[&page]
}

/// Which internal page a URL is, by exact match. Websites can't navigate a tab to a
/// `data:` URL, and no other page has this exact content.
pub fn internal_page_for_url(url: &str) -> Option<InternalPage> {
    InternalPage::ALL
        .into_iter()
        .find(|page| internal_url(*page) == url)
}
