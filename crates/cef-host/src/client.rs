//! Callbacks for the browsers in the window: the toolbar and one page per tab.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use bridge::{
    Download, DownloadList, DownloadState, InternalPage, KeyPress, PageCommand, SavedWindows,
    SearchEngine, Session, Settings, Shortcut, StartupMode, TabId, TabStrip, UiCommand, UiEvent,
    error_page, is_web_url, normalize_homepage, resolve_input, safe_file_name, shortcut_for,
    unique_path,
};
use cef::*;

use storage::Storage;

use crate::{
    app::open_window,
    data_dir,
    render::UI_COMMAND_MESSAGE,
    ui::{internal_page_for_url, internal_url},
};

const ERR_ABORTED: i32 = -3;
const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
const BLANK_PAGE: &str = "about:blank";
const PAGE_LIST_LIMIT: u32 = 500;
const SETTING_SEARCH_ENGINE: &str = "search_engine";
const SETTING_HOMEPAGE: &str = "homepage";
const SETTING_STARTUP: &str = "startup";
const SETTING_SESSION: &str = "session";
const TOOLBAR_HEIGHT: i32 = 88;
const DOWNLOAD_BAR_HEIGHT: i32 = 44;
const FIND_BAR_HEIGHT: i32 = 44;

#[cfg(target_os = "linux")]
type OsEvent = sys::XEvent;
#[cfg(target_os = "windows")]
type OsEvent = sys::MSG;

/// The parts of a tab that the toolbar doesn't need to know about.
struct TabHandles {
    view: BrowserView,
    browser: Option<Browser>,
    /// An error page to show once the failed load finishes.
    pending_error: Option<String>,
    /// The URL of the page on screen. `page.url` can be ahead of it during a load.
    committed_url: String,
    /// Whether the tab is showing an error page, whose title shouldn't reach history.
    showing_error: bool,
}

#[derive(Default)]
pub struct Shell {
    strip: TabStrip,
    handles: HashMap<TabId, TabHandles>,
    toolbar: Option<Browser>,
    toolbar_ready: bool,
    window: Option<Window>,
    layout: Option<BoxLayout>,
    open_browsers: usize,
    window_closing: bool,
    /// The tab reserved for a popup that CEF is about to create.
    pending_popup: Option<TabId>,
    downloads: DownloadList,
    /// How to cancel each active download.
    download_callbacks: HashMap<u32, DownloadItemCallback>,
    /// Whether the toolbar view currently has room for the download bar.
    download_bar_shown: bool,
    /// Restored tabs that haven't been selected yet, which have no browser and use no
    /// memory until then, and the address each one will load.
    unloaded: HashMap<TabId, String>,
    find_bar_shown: bool,
    find_text: String,
}

/// Data shared by every window.
struct SavedData {
    /// None if the database couldn't open. The browser still works, but saves nothing.
    storage: Option<Mutex<Storage>>,
    settings: Mutex<Settings>,
    /// The session as last saved, to avoid rewriting it when nothing changed.
    session: Mutex<String>,
}

fn saved_data() -> &'static SavedData {
    static DATA: OnceLock<SavedData> = OnceLock::new();
    DATA.get_or_init(|| {
        let storage = data_dir().and_then(|folder| {
            std::fs::create_dir_all(&folder).ok()?;
            match Storage::open(&folder.join("browser.db")) {
                Ok(storage) => Some(storage),
                Err(error) => {
                    eprintln!("Failed to open saved data: {error}");
                    None
                }
            }
        });
        let saved = |key| {
            storage
                .as_ref()
                .and_then(|storage| storage.setting(key).ok().flatten())
        };
        let defaults = Settings::default();
        let settings = Settings {
            search_engine: saved(SETTING_SEARCH_ENGINE)
                .and_then(|id| SearchEngine::from_id(&id))
                .unwrap_or(defaults.search_engine),
            homepage: saved(SETTING_HOMEPAGE).unwrap_or(defaults.homepage),
            startup: saved(SETTING_STARTUP)
                .and_then(|id| StartupMode::from_id(&id))
                .unwrap_or(defaults.startup),
        };
        SavedData {
            storage: storage.map(Mutex::new),
            settings: Mutex::new(settings),
            session: Mutex::default(),
        }
    })
}

fn settings() -> MutexGuard<'static, Settings> {
    saved_data()
        .settings
        .lock()
        .expect("Failed to lock settings")
}

/// Runs a saved-data write or read, logging failures rather than interrupting browsing.
fn with_storage<T>(action: impl FnOnce(&Storage) -> storage::Result<T>) -> Option<T> {
    let storage = saved_data().storage.as_ref()?;
    let storage = storage.lock().expect("Failed to lock saved data");
    action(&storage)
        .inspect_err(|error| eprintln!("Saved data error: {error}"))
        .ok()
}

impl Shell {
    fn with_storage<T>(&self, action: impl FnOnce(&Storage) -> storage::Result<T>) -> Option<T> {
        with_storage(action)
    }
}

fn find_import_sources() -> Vec<import::Source> {
    directories::UserDirs::new()
        .map(|dirs| import::find_sources(dirs.home_dir()))
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// The address to show for a URL, which for internal pages isn't their `data:` URL.
fn display_url(url: &str) -> String {
    internal_page_for_url(url).map_or_else(|| url.to_owned(), |page| page.address().to_owned())
}

pub type SharedShell = Arc<Mutex<Shell>>;

static WINDOWS: Mutex<Vec<SharedShell>> = Mutex::new(Vec::new());
/// Set when every window is closing together, so the session keeps them all.
static QUITTING: AtomicBool = AtomicBool::new(false);

fn open_windows() -> Vec<SharedShell> {
    WINDOWS.lock().expect("Failed to lock windows").clone()
}

pub fn register_shell(shell: &SharedShell) {
    WINDOWS
        .lock()
        .expect("Failed to lock windows")
        .push(shell.clone());
}

/// Closes every window, which ends the message loop.
pub fn close_all_browsers() {
    QUITTING.store(true, Ordering::Relaxed);
    for shell in open_windows() {
        close_window(&shell);
    }
}

// CEF calls can run callbacks on this thread before returning, and those callbacks
// take this lock. So never hold the guard across a CEF call: copy out what's needed.
fn lock(shell: &SharedShell) -> MutexGuard<'_, Shell> {
    shell.lock().expect("Failed to lock shell")
}

fn send_to_toolbar(toolbar: Option<&Browser>, event: &UiEvent) {
    let Some(frame) = toolbar.and_then(|toolbar| toolbar.main_frame()) else {
        return;
    };
    // serde_json escapes page-controlled strings, such as URLs and titles.
    let json = serde_json::to_string(event).expect("Failed to serialize event");
    let code = CefString::from(format!("window.onBrowserEvent({json})").as_str());
    frame.execute_java_script(Some(&code), None, 0);
}

fn publish(shell: &SharedShell) {
    let (toolbar, strip) = {
        let shell = lock(shell);
        (
            shell.toolbar.clone().filter(|_| shell.toolbar_ready),
            shell.strip.clone(),
        )
    };
    send_to_toolbar(toolbar.as_ref(), &UiEvent::Tabs { strip: &strip });
    save_session();
}

/// Saves the open tabs whenever they change, so a crash loses as little as possible.
/// When the last window closes it isn't saved over, so its tabs reopen next time.
fn save_session() {
    if QUITTING.load(Ordering::Relaxed) {
        return;
    }
    let windows: Vec<Session> = open_windows()
        .iter()
        .filter_map(|shell| {
            let shell = lock(shell);
            (!shell.window_closing).then(|| Session::from_strip(&shell.strip))
        })
        .collect();
    if windows.is_empty() {
        return;
    }
    let saved = SavedWindows { windows };
    let json = serde_json::to_string(&saved).expect("Failed to serialize session");
    let mut last = saved_data().session.lock().expect("Failed to lock session");
    if *last != json {
        with_storage(|storage| storage.set_setting(SETTING_SESSION, &json));
        *last = json;
    }
}

/// The windows to reopen at startup, if any.
pub fn startup_windows() -> Vec<Session> {
    if settings().startup != StartupMode::RestoreTabs {
        return Vec::new();
    }
    with_storage(|storage| storage.setting(SETTING_SESSION))
        .flatten()
        .map_or_else(Vec::new, |json| SavedWindows::parse(&json).windows)
}

/// Sets up the tabs to show at startup and returns the one to select first. Nothing
/// loads yet: each tab creates its browser when it's first selected.
pub fn prepare_tabs(
    shell: &SharedShell,
    session: Option<Session>,
    requested_url: Option<&str>,
) -> TabId {
    let mut guard = lock(shell);
    let session = session.unwrap_or_default();

    let add = |shell: &mut Shell, url: &str, title: &str| {
        let id = shell.strip.add();
        let is_bookmarked = shell
            .with_storage(|storage| storage.is_bookmarked(url))
            .unwrap_or(false);
        if let Some(tab) = shell.strip.get_mut(id) {
            tab.page.url = url.to_owned();
            tab.page.is_bookmarked = is_bookmarked;
            tab.title = title.to_owned();
        }
        shell.unloaded.insert(id, url.to_owned());
        id
    };

    let ids: Vec<TabId> = session
        .tabs
        .iter()
        .map(|tab| add(&mut guard, &tab.url, &tab.title))
        .collect();
    if let Some(url) = requested_url {
        return add(&mut guard, url, "");
    }
    if let Some(id) = ids.get(session.active) {
        return *id;
    }
    let homepage = settings().homepage.clone();
    add(&mut guard, &homepage, "")
}

/// The toolbar view's height, which includes any bars that are showing.
pub fn toolbar_height(shell: &SharedShell) -> i32 {
    let shell = lock(shell);
    TOOLBAR_HEIGHT
        + if shell.download_bar_shown {
            DOWNLOAD_BAR_HEIGHT
        } else {
            0
        }
        + if shell.find_bar_shown {
            FIND_BAR_HEIGHT
        } else {
            0
        }
}

/// Makes the window lay out the toolbar view again, after a bar appeared or went away.
fn resize_toolbar(shell: &SharedShell) {
    let (toolbar, window) = {
        let shell = lock(shell);
        (shell.toolbar.clone(), shell.window.clone())
    };
    if let (Some(mut toolbar), Some(window)) = (toolbar, window) {
        if let Some(view) = browser_view_get_for_browser(Some(&mut toolbar)) {
            View::from(&view).invalidate_layout();
        }
        window.layout();
    }
}

/// Shows the download list, resizing the toolbar view if the bar appeared or went away.
fn publish_downloads(shell: &SharedShell) {
    let (toolbar, items, resized) = {
        let mut shell = lock(shell);
        let show_bar = !shell.downloads.items().is_empty();
        let resized = show_bar != shell.download_bar_shown;
        shell.download_bar_shown = show_bar;
        (
            shell.toolbar.clone().filter(|_| shell.toolbar_ready),
            shell.downloads.items().to_vec(),
            resized,
        )
    };
    send_to_toolbar(toolbar.as_ref(), &UiEvent::Downloads { items: &items });
    if resized {
        resize_toolbar(shell);
    }
}

fn focus_toolbar(toolbar: &Browser) {
    let mut browser = toolbar.clone();
    if let Some(view) = browser_view_get_for_browser(Some(&mut browser)) {
        view.request_focus();
    }
}

fn open_find(shell: &SharedShell) {
    let (toolbar, was_shown) = {
        let mut shell = lock(shell);
        let was_shown = shell.find_bar_shown;
        shell.find_bar_shown = true;
        (shell.toolbar.clone(), was_shown)
    };
    if !was_shown {
        resize_toolbar(shell);
    }
    if let Some(toolbar) = toolbar {
        focus_toolbar(&toolbar);
        send_to_toolbar(Some(&toolbar), &UiEvent::ShowFind);
    }
}

fn run_find(shell: &SharedShell, text: &str, forward: bool, again: bool) {
    lock(shell).find_text = text.to_owned();
    let Some(host) = active_browser(shell).and_then(|browser| browser.host()) else {
        return;
    };
    if text.is_empty() {
        host.stop_finding(1);
        let toolbar = lock(shell).toolbar.clone();
        send_to_toolbar(
            toolbar.as_ref(),
            &UiEvent::FindResult {
                count: 0,
                active: 0,
            },
        );
    } else {
        host.find(
            Some(&CefString::from(text)),
            forward.into(),
            0,
            again.into(),
        );
    }
}

fn close_find(shell: &SharedShell, focus_page: bool) {
    let toolbar = {
        let mut shell = lock(shell);
        shell.find_bar_shown = false;
        shell.toolbar.clone()
    };
    let page = active_browser(shell);
    if let Some(host) = page.as_ref().and_then(|browser| browser.host()) {
        host.stop_finding(1);
    }
    send_to_toolbar(toolbar.as_ref(), &UiEvent::HideFind);
    resize_toolbar(shell);
    if focus_page
        && let Some(mut page) = page
        && let Some(view) = browser_view_get_for_browser(Some(&mut page))
    {
        view.request_focus();
    }
}

fn downloads_folder() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| {
            dirs.download_dir()
                .map(PathBuf::from)
                .or_else(|| Some(dirs.home_dir().join("Downloads")))
        })
        .unwrap_or_else(std::env::temp_dir)
}

/// Opens the file manager at a download. Nothing here ever runs the downloaded file.
fn show_in_folder(path: &Path) {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(format!("/select,{}", path.display()));
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        command
    };
    if let Ok(mut child) = command.spawn() {
        // Collect the exit status so the child doesn't linger.
        std::thread::spawn(move || child.wait());
    }
}

/// A download link that opened a new tab would leave it blank. Its browser must live
/// until the download ends to keep reporting progress, so take it off the strip for now.
fn hide_blank_download_tab(shell: &SharedShell, id: TabId) {
    let next = {
        let mut shell = lock(shell);
        let is_blank = shell
            .strip
            .get(id)
            .is_some_and(|tab| tab.page.url.is_empty() || tab.page.url == BLANK_PAGE);
        if !is_blank || shell.strip.tabs().len() <= 1 {
            return;
        }
        shell.strip.remove(id);
        shell.strip.active_id()
    };
    if let Some(next) = next {
        select_tab(shell, next);
    }
}

/// Closes a tab hidden for a download, now that the download has ended.
fn close_hidden_tab(shell: &SharedShell, id: TabId) {
    let browser = {
        let shell = lock(shell);
        if shell.strip.get(id).is_some() {
            return;
        }
        shell
            .handles
            .get(&id)
            .and_then(|handles| handles.browser.clone())
    };
    if let Some(host) = browser.and_then(|browser| browser.host()) {
        host.close_browser(0);
    }
}

fn update_tab(shell: &SharedShell, id: TabId, change: impl FnOnce(&mut bridge::Tab)) {
    if let Some(tab) = lock(shell).strip.get_mut(id) {
        change(tab);
    }
    publish(shell);
}

/// Reserves a tab and creates its view. The view shows once it's attached to the window.
pub fn create_tab(shell: &SharedShell, url: &str) -> TabId {
    let id = lock(shell).strip.add();
    create_view(shell, id, url);
    id
}

fn create_view(shell: &SharedShell, id: TabId, url: &str) {
    let mut client = PageClient::new(shell.clone(), id);
    let view = browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        None,
        Some(&mut PageViewHost::new(shell.clone())),
    )
    .expect("Failed to create browser view");
    store_view(shell, id, view);
}

fn store_view(shell: &SharedShell, id: TabId, view: BrowserView) {
    lock(shell).handles.insert(
        id,
        TabHandles {
            view,
            browser: None,
            pending_error: None,
            committed_url: String::new(),
            showing_error: false,
        },
    );
}

/// Adds a tab's view to the window and shows it. Called once the window exists.
pub fn attach_tab(shell: &SharedShell, id: TabId) {
    let (window, layout, view) = {
        let shell = lock(shell);
        (
            shell.window.clone(),
            shell.layout.clone(),
            shell.handles.get(&id).map(|handles| handles.view.clone()),
        )
    };
    if let (Some(window), Some(view)) = (window, view) {
        let mut view = View::from(&view);
        window.add_child_view(Some(&mut view));
        if let Some(layout) = layout {
            layout.set_flex_for_view(Some(&mut view), 1);
        }
    }
    select_tab(shell, id);
}

pub fn set_window(shell: &SharedShell, window: Window, layout: Option<BoxLayout>) {
    let mut shell = lock(shell);
    shell.window = Some(window);
    shell.layout = layout;
}

/// Shows a tab, hides the others and moves focus to it.
pub fn select_tab(shell: &SharedShell, id: TabId) {
    let (should_close_find, unloaded) = {
        let mut shell = lock(shell);
        let closes_find = shell.find_bar_shown && shell.strip.active_id() != Some(id);
        (closes_find, shell.unloaded.remove(&id))
    };
    if should_close_find {
        close_find(shell, false);
    }
    // A restored tab creates its browser the first time it's shown.
    if let Some(address) = unloaded {
        let url = InternalPage::from_address(&address)
            .map_or(address.clone(), |page| internal_url(page).to_owned());
        create_view(shell, id, &url);
        attach_tab(shell, id);
        return;
    }

    let (views, window, title, url) = {
        let mut shell = lock(shell);
        if !shell.strip.select(id) {
            return;
        }
        let views: Vec<(TabId, BrowserView)> = shell
            .handles
            .iter()
            .map(|(id, handles)| (*id, handles.view.clone()))
            .collect();
        let tab = shell.strip.get(id);
        (
            views,
            shell.window.clone(),
            tab.map(|tab| tab.title.clone()).unwrap_or_default(),
            tab.map(|tab| tab.page.url.clone()).unwrap_or_default(),
        )
    };
    for (tab_id, view) in &views {
        View::from(view).set_visible((*tab_id == id).into());
    }
    if let Some(window) = window {
        window.layout();
        window.set_title(Some(&CefString::from(title.as_str())));
    }
    if url.is_empty() || url == BLANK_PAGE {
        focus_address(shell);
    } else if let Some((_, view)) = views.iter().find(|(tab_id, _)| *tab_id == id) {
        view.request_focus();
    }
    publish(shell);
}

fn focus_address(shell: &SharedShell) {
    let toolbar = lock(shell).toolbar.clone();
    if let Some(toolbar) = toolbar {
        let mut browser = toolbar.clone();
        if let Some(view) = browser_view_get_for_browser(Some(&mut browser)) {
            view.request_focus();
        }
        send_to_toolbar(Some(&toolbar), &UiEvent::FocusAddress);
    }
}

fn new_tab(shell: &SharedShell) {
    let id = create_tab(shell, BLANK_PAGE);
    attach_tab(shell, id);
}

/// Closes a tab, or the window if it's the last one.
fn close_tab(shell: &SharedShell, id: TabId) {
    let (is_last, is_unloaded, browser) = {
        let shell = lock(shell);
        (
            shell.strip.tabs().len() <= 1,
            shell.unloaded.contains_key(&id),
            shell
                .handles
                .get(&id)
                .and_then(|handles| handles.browser.clone()),
        )
    };
    if is_last {
        close_window(shell);
    } else if is_unloaded {
        lock(shell).unloaded.remove(&id);
        remove_tab(shell, id);
    } else if let Some(host) = browser.and_then(|browser| browser.host()) {
        // This runs the page's unload handlers, then calls `do_close`.
        host.close_browser(0);
    }
}

fn close_window(shell: &SharedShell) {
    let window = lock(shell).window.clone();
    if let Some(window) = window {
        window.close();
    }
}

/// Removes a closing tab from the window and shows its neighbor.
fn remove_tab(shell: &SharedShell, id: TabId) {
    let (window, view, next) = {
        let mut shell = lock(shell);
        // Dropping these references is what lets the browser finish closing.
        let view = shell.handles.remove(&id).map(|handles| handles.view);
        shell.strip.remove(id);
        (shell.window.clone(), view, shell.strip.active_id())
    };
    if let (Some(window), Some(view)) = (window, view) {
        window.remove_child_view(Some(&mut View::from(&view)));
    }
    if let Some(next) = next {
        select_tab(shell, next);
    }
}

fn active_browser(shell: &SharedShell) -> Option<Browser> {
    let shell = lock(shell);
    let id = shell.strip.active_id()?;
    shell.handles.get(&id)?.browser.clone()
}

fn run_command(shell: &SharedShell, command: UiCommand) {
    match command {
        UiCommand::Ready => {
            lock(shell).toolbar_ready = true;
            publish(shell);
            publish_downloads(shell);
            // A new window's first tab was shown before its toolbar could take focus.
            let is_blank = lock(shell)
                .strip
                .active()
                .is_some_and(|tab| tab.page.url.is_empty() || tab.page.url == BLANK_PAGE);
            if is_blank {
                focus_address(shell);
            }
        }
        UiCommand::CancelDownload { id } => {
            let callback = lock(shell).download_callbacks.get(&id).cloned();
            if let Some(callback) = callback {
                callback.cancel();
            }
        }
        UiCommand::DismissDownload { id } => {
            lock(shell).downloads.dismiss(id);
            publish_downloads(shell);
        }
        UiCommand::ClearDownloads => {
            lock(shell).downloads.clear_finished();
            publish_downloads(shell);
        }
        UiCommand::ShowDownload { id } => {
            // The toolbar only sends an ID, so it can't name an arbitrary path.
            let path = lock(shell).downloads.get(id).map(|item| item.path.clone());
            if let Some(path) = path {
                show_in_folder(&path);
            }
        }
        UiCommand::NewTab => new_tab(shell),
        UiCommand::ToggleBookmark => toggle_bookmark(shell),
        UiCommand::Find {
            text,
            forward,
            again,
        } => run_find(shell, &text, forward, again),
        UiCommand::CloseFind => close_find(shell, true),
        UiCommand::OpenPage { page } => open_page(shell, page),
        UiCommand::SelectTab { id } => select_tab(shell, id),
        UiCommand::CloseTab { id } => close_tab(shell, id),
        UiCommand::MoveTab { id, index } => {
            lock(shell).strip.move_to(id, index);
            publish(shell);
        }
        UiCommand::Navigate { input } => navigate(shell, &input),
        UiCommand::Back | UiCommand::Forward | UiCommand::Reload | UiCommand::Stop => {
            let Some(page) = active_browser(shell) else {
                return;
            };
            match command {
                UiCommand::Back => page.go_back(),
                UiCommand::Forward => page.go_forward(),
                UiCommand::Reload => page.reload(),
                _ => page.stop_load(),
            }
        }
    }
}

fn navigate(shell: &SharedShell, input: &str) {
    let search_prefix = settings().search_engine.search_url();
    let internal = InternalPage::from_address(input);
    let url = match internal {
        Some(page) => Some(internal_url(page).to_owned()),
        None => resolve_input(input, search_prefix),
    };
    let (Some(url), Some(page)) = (url, active_browser(shell)) else {
        return;
    };
    let (id, view) = {
        let shell = lock(shell);
        let id = shell.strip.active_id();
        (
            id,
            id.and_then(|id| shell.handles.get(&id))
                .map(|handles| handles.view.clone()),
        )
    };
    if let Some(id) = id {
        update_tab(shell, id, |tab| tab.page.url = display_url(&url));
    }
    if let Some(frame) = page.main_frame() {
        frame.load_url(Some(&CefString::from(url.as_str())));
    }
    if let Some(view) = view {
        view.request_focus();
    }
}

/// Shows an internal page, reusing a blank tab or else opening a new one.
fn open_page(shell: &SharedShell, page: InternalPage) {
    let is_blank = {
        let shell = lock(shell);
        shell
            .strip
            .active()
            .is_some_and(|tab| tab.page.url.is_empty() || tab.page.url == BLANK_PAGE)
    };
    if is_blank {
        navigate(shell, page.address());
    } else {
        let id = create_tab(shell, internal_url(page));
        attach_tab(shell, id);
    }
}

fn toggle_bookmark(shell: &SharedShell) {
    {
        let shell = lock(shell);
        let Some(tab) = shell.strip.active().filter(|tab| is_web_url(&tab.page.url)) else {
            return;
        };
        let (url, title) = (tab.page.url.clone(), tab.title.clone());
        if tab.page.is_bookmarked {
            with_storage(|storage| storage.remove_bookmark(&url));
        } else {
            with_storage(|storage| storage.add_bookmark(&url, &title, now_ms()));
        }
    }
    refresh_bookmark_stars();
}

/// Updates the star on every tab in every window, after a bookmark changed.
fn refresh_bookmark_stars() {
    for shell in open_windows() {
        {
            let mut guard = lock(&shell);
            let tabs: Vec<(TabId, String)> = guard
                .strip
                .tabs()
                .iter()
                .map(|tab| (tab.id, tab.page.url.clone()))
                .collect();
            for (id, url) in tabs {
                let marked = with_storage(|storage| storage.is_bookmarked(&url)).unwrap_or(false);
                if let Some(tab) = guard.strip.get_mut(id) {
                    tab.page.is_bookmarked = marked;
                }
            }
        }
        publish(&shell);
    }
}

fn run_page_command(shell: &SharedShell, frame: &Frame, command: PageCommand) {
    let reply = |event: serde_json::Value| {
        let code = format!("window.onBrowserEvent({event})");
        frame.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
    };
    match command {
        PageCommand::ListHistory { query } => {
            let items = lock(shell)
                .with_storage(|storage| storage.history(&query, PAGE_LIST_LIMIT))
                .unwrap_or_default();
            reply(serde_json::json!({ "type": "history", "items": items }));
        }
        PageCommand::DeleteHistory { url } => {
            lock(shell).with_storage(|storage| storage.delete_history_entry(&url));
        }
        PageCommand::ClearHistory => {
            lock(shell).with_storage(Storage::clear_history);
        }
        PageCommand::ListBookmarks { query } => {
            let items = lock(shell)
                .with_storage(|storage| storage.bookmarks(&query, PAGE_LIST_LIMIT))
                .unwrap_or_default();
            reply(serde_json::json!({ "type": "bookmarks", "items": items }));
        }
        PageCommand::RemoveBookmark { url } => {
            lock(shell).with_storage(|storage| storage.remove_bookmark(&url));
            refresh_bookmark_stars();
        }
        PageCommand::GetSettings => {
            let current = settings().clone();
            reply(serde_json::json!({ "type": "settings", "settings": current }));
        }
        PageCommand::SetSearchEngine { engine } => {
            settings().search_engine = engine;
            with_storage(|storage| storage.set_setting(SETTING_SEARCH_ENGINE, engine.id()));
        }
        PageCommand::ListImportSources => {
            let sources = find_import_sources();
            reply(serde_json::json!({ "type": "importSources", "sources": sources }));
        }
        PageCommand::Import { source } => {
            // The page only names a source this browser found, never a path.
            let found = find_import_sources()
                .into_iter()
                .find(|item| item.id == source);
            let result = found.map(|found| {
                let data = import::read_source(&found);
                let shell = lock(shell);
                let bookmarks = shell.with_storage(|storage| {
                    storage.import_bookmarks(
                        data.bookmarks
                            .iter()
                            .map(|item| (item.url.as_str(), item.title.as_str(), item.time_ms)),
                    )
                });
                let history = shell.with_storage(|storage| {
                    storage.import_history(data.history.iter().map(|item| {
                        (
                            item.url.as_str(),
                            item.title.as_str(),
                            item.time_ms,
                            item.visit_count,
                        )
                    }))
                });
                (bookmarks.unwrap_or(0), history.unwrap_or(0))
            });
            refresh_bookmark_stars();
            let (bookmarks, history) = result.unwrap_or((0, 0));
            reply(serde_json::json!({
                "type": "importResult",
                "bookmarks": bookmarks,
                "history": history,
            }));
        }
        PageCommand::SetStartup { startup } => {
            settings().startup = startup;
            with_storage(|storage| storage.set_setting(SETTING_STARTUP, startup.id()));
        }
        PageCommand::SetHomepage { homepage } => {
            let normalized = normalize_homepage(&homepage);
            let saved = normalized.is_some();
            if let Some(url) = normalized {
                with_storage(|storage| storage.set_setting(SETTING_HOMEPAGE, &url));
                settings().homepage = url;
            }
            reply(serde_json::json!({ "type": "homepageResult", "saved": saved }));
            if saved {
                let current = settings().clone();
                reply(serde_json::json!({ "type": "settings", "settings": current }));
            }
        }
    }
}

/// Returns whether the key press was used.
fn run_shortcut(shell: &SharedShell, event: &KeyEvent) -> bool {
    if event.type_ != KeyEventType::RAWKEYDOWN {
        return false;
    }
    let key = KeyPress {
        key_code: event.windows_key_code,
        ctrl: event.modifiers & EVENTFLAG_CONTROL_DOWN != 0,
        shift: event.modifiers & EVENTFLAG_SHIFT_DOWN != 0,
        alt: event.modifiers & EVENTFLAG_ALT_DOWN != 0,
        command: event.modifiers & EVENTFLAG_COMMAND_DOWN != 0,
    };
    let Some(shortcut) = shortcut_for(key, cfg!(target_os = "macos")) else {
        return false;
    };

    let (page, strip) = (active_browser(shell), lock(shell).strip.clone());
    let select = |id: Option<TabId>| {
        if let Some(id) = id {
            select_tab(shell, id);
        }
    };
    match shortcut {
        Shortcut::FocusAddress => focus_address(shell),
        Shortcut::NewTab => new_tab(shell),
        Shortcut::Find => open_find(shell),
        Shortcut::FindNext | Shortcut::FindPrevious => {
            let text = lock(shell).find_text.clone();
            open_find(shell);
            run_find(shell, &text, shortcut == Shortcut::FindNext, true);
        }
        Shortcut::Stop if lock(shell).find_bar_shown => close_find(shell, true),
        Shortcut::ToggleBookmark => toggle_bookmark(shell),
        Shortcut::History => open_page(shell, InternalPage::History),
        Shortcut::Bookmarks => open_page(shell, InternalPage::Bookmarks),
        Shortcut::Settings => open_page(shell, InternalPage::Settings),
        Shortcut::Quit => close_all_browsers(),
        Shortcut::NewWindow => open_window(None, Some(BLANK_PAGE)),
        Shortcut::CloseTab => {
            if let Some(id) = strip.active_id() {
                close_tab(shell, id);
            }
        }
        Shortcut::NextTab => select(strip.neighbor(1)),
        Shortcut::PreviousTab => select(strip.neighbor(-1)),
        Shortcut::TabAt(index) => select(strip.at(index)),
        Shortcut::LastTab => select(strip.last()),
        // Escape stays with the page and address bar unless there's a load to stop.
        Shortcut::Stop if !strip.active().is_some_and(|tab| tab.page.is_loading) => return false,
        Shortcut::Back
        | Shortcut::Forward
        | Shortcut::Reload
        | Shortcut::HardReload
        | Shortcut::Stop => {
            let Some(page) = page else {
                return false;
            };
            match shortcut {
                Shortcut::Back => page.go_back(),
                Shortcut::Forward => page.go_forward(),
                Shortcut::Reload => page.reload(),
                Shortcut::HardReload => page.reload_ignore_cache(),
                _ => page.stop_load(),
            }
        }
    }
    true
}

/// Asks every browser to close for the window closing. Returns whether they all have.
pub fn try_close_all(shell: &SharedShell) -> bool {
    let (toolbar, pages) = {
        let mut shell = lock(shell);
        shell.window_closing = true;
        let pages: Vec<Browser> = shell
            .handles
            .values()
            .filter_map(|handles| handles.browser.clone())
            .collect();
        (shell.toolbar.clone(), pages)
    };
    // Ask them all, even after one says no, so each starts closing.
    let closed: Vec<bool> = pages
        .into_iter()
        .chain(toolbar)
        .filter_map(|browser| browser.host())
        .map(|host| host.try_close_browser() != 0)
        .collect();
    closed.into_iter().all(|closed| closed)
}

pub fn clear_window(shell: &SharedShell) {
    let mut shell = lock(shell);
    shell.window = None;
    shell.layout = None;
    for handles in shell.handles.values_mut() {
        handles.browser = None;
    }
}

fn browser_closed(shell: &SharedShell, tab: Option<TabId>) {
    let window_closed = {
        let mut shell = lock(shell);
        if let Some(tab) = tab {
            shell.handles.remove(&tab);
        } else {
            shell.toolbar = None;
        }
        shell.open_browsers = shell.open_browsers.saturating_sub(1);
        shell.open_browsers == 0
    };
    if !window_closed {
        return;
    }
    lock(shell).handles.clear();
    let no_windows_left = {
        let mut windows = WINDOWS.lock().expect("Failed to lock windows");
        windows.retain(|window| !Arc::ptr_eq(window, shell));
        windows.is_empty()
    };
    if no_windows_left {
        quit_message_loop();
    } else {
        save_session();
    }
}

wrap_keyboard_handler! {
    struct Shortcuts {
        shell: SharedShell,
    }

    impl KeyboardHandler {
        #[cfg(not(target_os = "macos"))]
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut OsEvent>,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            event.is_some_and(|event| run_shortcut(&self.shell, event)).into()
        }

        #[cfg(target_os = "macos")]
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            event.is_some_and(|event| run_shortcut(&self.shell, event)).into()
        }
    }
}

// The toolbar's client. It's the only one that accepts bridge commands.
wrap_client! {
    pub struct ToolbarClient {
        shell: SharedShell,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ToolbarLifeSpan::new(self.shell.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(Shortcuts::new(self.shell.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(ToolbarRequests::new())
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            let Some(message) = message else {
                return 0;
            };
            if CefString::from(&message.name()).to_string() != UI_COMMAND_MESSAGE {
                return 0;
            }
            let json = message
                .argument_list()
                .map(|list| CefString::from(&list.string(0)).to_string())
                .unwrap_or_default();
            if let Ok(command) = serde_json::from_str(&json) {
                run_command(&self.shell, command);
            }
            1
        }
    }
}

wrap_life_span_handler! {
    struct ToolbarLifeSpan {
        shell: SharedShell,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let mut shell = lock(&self.shell);
            shell.toolbar = browser.cloned();
            shell.open_browsers += 1;
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed(&self.shell, None);
        }
    }
}

wrap_request_handler! {
    struct ToolbarRequests {}

    impl RequestHandler {
        // The toolbar only ever shows its own page. Refuse anything else, such as a
        // dropped link.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let url = request.map(|request| CefString::from(&request.url()).to_string());
            (!url.is_some_and(|url| url.starts_with("data:text/html;base64,"))).into()
        }
    }
}

wrap_browser_view_delegate! {
    pub struct PageViewHost {
        shell: SharedShell,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1280, height: 712 }
        }
    }

    impl BrowserViewDelegate {
        // Popups become ordinary tabs, keeping their link to the page that opened them.
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: i32,
        ) -> i32 {
            let id = lock(&self.shell).pending_popup.take();
            let (Some(id), Some(view)) = (id, popup_browser_view) else {
                return 0;
            };
            store_view(&self.shell, id, view.clone());
            attach_tab(&self.shell, id);
            1
        }

        // Every browser in a window must share a style, and Alloy draws no browser UI
        // of its own.
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_client! {
    pub struct PageClient {
        shell: SharedShell,
        id: TabId,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(PageDisplay::new(self.shell.clone(), self.id))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(PageLifeSpan::new(self.shell.clone(), self.id))
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(PageLoad::new(self.shell.clone(), self.id))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(Shortcuts::new(self.shell.clone()))
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            Some(DenyPermissions::new())
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(PageDownloads::new(self.shell.clone(), self.id))
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(PageFind::new(self.shell.clone(), self.id))
        }

        // Every page can send this message, so the browser process decides who is
        // trusted: only the main frame of a tab committed to an internal page's exact URL.
        fn on_process_message_received(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> i32 {
            let (Some(frame), Some(message)) = (frame, message) else {
                return 0;
            };
            if CefString::from(&message.name()).to_string() != UI_COMMAND_MESSAGE {
                return 0;
            }
            let url = CefString::from(&frame.url()).to_string();
            if frame.is_main() == 0 || internal_page_for_url(&url).is_none() {
                return 1;
            }
            let json = message
                .argument_list()
                .map(|list| CefString::from(&list.string(0)).to_string())
                .unwrap_or_default();
            if let Ok(command) = serde_json::from_str(&json) {
                run_page_command(&self.shell, frame, command);
            }
            1
        }
    }
}

wrap_find_handler! {
    struct PageFind {
        shell: SharedShell,
        id: TabId,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: i32,
            count: i32,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: i32,
            _final_update: i32,
        ) {
            let toolbar = {
                let shell = lock(&self.shell);
                let is_active = shell.strip.active_id() == Some(self.id);
                shell.toolbar.clone().filter(|_| is_active && shell.find_bar_shown)
            };
            send_to_toolbar(
                toolbar.as_ref(),
                &UiEvent::FindResult { count, active: active_match_ordinal },
            );
        }
    }
}

wrap_download_handler! {
    struct PageDownloads {
        shell: SharedShell,
        id: TabId,
    }

    impl DownloadHandler {
        // The binding's default refuses every download.
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> i32 {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> i32 {
            let (Some(item), Some(callback)) = (download_item, callback) else {
                return 0;
            };
            let id = item.id();

            // Chromium retries an interrupted download under the same ID, so keep its
            // entry and path.
            let existing = lock(&self.shell).downloads.get(id).map(|item| item.path.clone());
            let path = match existing {
                Some(path) => path,
                None => {
                    let suggested = suggested_name.map(CefString::to_string).unwrap_or_default();
                    let file_name = safe_file_name(&suggested);
                    let folder = downloads_folder();
                    if std::fs::create_dir_all(&folder).is_err() {
                        return 0;
                    }
                    // Downloads in other windows count too.
                    let windows = open_windows();
                    let path = unique_path(&folder, &file_name, |path| {
                        path.exists()
                            || windows.iter().any(|window| lock(window).downloads.claims(path))
                    });
                    let mut shell = lock(&self.shell);
                    shell.downloads.add(Download {
                        id,
                        file_name: path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or(file_name),
                        path: path.clone(),
                        state: DownloadState::InProgress,
                        received_bytes: item.received_bytes(),
                        total_bytes: item.total_bytes(),
                        bytes_per_second: 0,
                    });
                    path
                }
            };
            callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), 0);
            publish_downloads(&self.shell);
            hide_blank_download_tab(&self.shell, self.id);
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else {
                return;
            };
            let state = if item.is_complete() != 0 {
                DownloadState::Complete
            } else if item.is_canceled() != 0 {
                DownloadState::Cancelled
            } else if item.is_interrupted() != 0 {
                DownloadState::Failed
            } else {
                DownloadState::InProgress
            };
            {
                let mut shell = lock(&self.shell);
                let id = item.id();
                shell.downloads.update(
                    id,
                    state,
                    item.received_bytes(),
                    item.total_bytes(),
                    item.current_speed(),
                );
                match (state, callback) {
                    (DownloadState::InProgress, Some(callback)) => {
                        shell.download_callbacks.insert(id, callback.clone());
                    }
                    _ => {
                        shell.download_callbacks.remove(&id);
                    }
                }
            }
            publish_downloads(&self.shell);
            if state != DownloadState::InProgress {
                close_hidden_tab(&self.shell, self.id);
            }
        }
    }
}

// There's no permission prompt UI yet. Without an answer, pages wait forever.
wrap_permission_handler! {
    struct DenyPermissions {}

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            if let Some(callback) = callback {
                callback.cancel();
            }
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            _prompt_id: u64,
            _requesting_origin: Option<&CefString>,
            _requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            if let Some(callback) = callback {
                callback.cont(PermissionRequestResult::DENY);
            }
            1
        }
    }
}

wrap_display_handler! {
    struct PageDisplay {
        shell: SharedShell,
        id: TabId,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if !frame.is_some_and(|frame| frame.is_main() != 0) {
                return;
            }
            let url = display_url(&url.map(CefString::to_string).unwrap_or_default());
            let is_bookmarked = {
                let mut shell = lock(&self.shell);
                if let Some(handles) = shell.handles.get_mut(&self.id) {
                    handles.committed_url = url.clone();
                    handles.showing_error = false;
                }
                // Page loads are recorded when they finish, in case they fail. This is
                // for sites that change their address without loading a new page.
                let is_loading = shell.strip.get(self.id).is_some_and(|tab| tab.page.is_loading);
                if !is_loading && is_web_url(&url) {
                    shell.with_storage(|storage| storage.record_visit(&url, "", now_ms()));
                }
                shell
                    .with_storage(|storage| storage.is_bookmarked(&url))
                    .unwrap_or(false)
            };
            update_tab(&self.shell, self.id, |tab| {
                tab.page.url = url;
                tab.page.is_bookmarked = is_bookmarked;
            });
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let (window, is_active) = {
                let shell = lock(&self.shell);
                (shell.window.clone(), shell.strip.active_id() == Some(self.id))
            };
            if is_active && let Some(window) = window {
                let is_blank = title.is_some_and(|title| title.to_string() == BLANK_PAGE);
                let new_tab = CefString::from("New tab");
                window.set_title(if is_blank { Some(&new_tab) } else { title });
            }
            let title = title.map(CefString::to_string).unwrap_or_default();
            {
                let shell = lock(&self.shell);
                let showing_error = shell.handles.get(&self.id).is_some_and(|handles| handles.showing_error);
                if let Some(tab) = shell.strip.get(self.id).filter(|tab| !showing_error && is_web_url(&tab.page.url)) {
                    let url = tab.page.url.clone();
                    shell.with_storage(|storage| storage.set_history_title(&url, &title));
                }
            }
            update_tab(&self.shell, self.id, |tab| tab.title = title);
        }

        fn on_loading_progress_change(&self, _browser: Option<&mut Browser>, progress: f64) {
            update_tab(&self.shell, self.id, |tab| tab.page.progress = progress);
        }
    }
}

wrap_life_span_handler! {
    struct PageLifeSpan {
        shell: SharedShell,
        id: TabId,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let mut shell = lock(&self.shell);
            shell.open_browsers += 1;
            if let Some(handles) = shell.handles.get_mut(&self.id) {
                handles.browser = browser.cloned();
            }
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            // Pages can't open tabs on their own, only in response to a click or key.
            let Some(client) = client.filter(|_| user_gesture != 0) else {
                return 1;
            };
            let mut shell = lock(&self.shell);
            let id = shell.strip.add();
            shell.pending_popup = Some(id);
            *client = Some(PageClient::new(self.shell.clone(), id));
            0
        }

        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, _popup_id: i32) {
            let mut shell = lock(&self.shell);
            if let Some(id) = shell.pending_popup.take() {
                shell.strip.remove(id);
            }
        }

        // Returning 1 keeps the window open when one tab closes. Without it, a browser
        // hosted in a window closes that window.
        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            if lock(&self.shell).window_closing {
                return 0;
            }
            let closes_window = {
                let shell = lock(&self.shell);
                let tabs = shell.strip.tabs();
                tabs.len() <= 1 && tabs.iter().any(|tab| tab.id == self.id)
            };
            if closes_window {
                close_window(&self.shell);
                return 1;
            }
            remove_tab(&self.shell, self.id);
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed(&self.shell, Some(self.id));
        }
    }
}

wrap_load_handler! {
    struct PageLoad {
        shell: SharedShell,
        id: TabId,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: i32,
            can_go_back: i32,
            can_go_forward: i32,
        ) {
            let committed_url = lock(&self.shell)
                .handles
                .get(&self.id)
                .map(|handles| handles.committed_url.clone())
                .unwrap_or_default();
            update_tab(&self.shell, self.id, |tab| {
                tab.page.is_loading = is_loading != 0;
                tab.page.can_go_back = can_go_back != 0;
                tab.page.can_go_forward = can_go_forward != 0;
                if is_loading != 0 {
                    tab.page.progress = 0.0;
                } else {
                    // A stopped load never commits, so go back to the page on screen.
                    tab.page.url = committed_url;
                }
            });
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if !frame.is_some_and(|frame| frame.is_main() != 0) {
                return;
            }
            // Downloads and stopped loads report this. They aren't failures.
            let error_code = sys::cef_errorcode_t::from(error_code) as i32;
            if error_code == ERR_ABORTED {
                return;
            }
            let failed_url = failed_url.map(CefString::to_string).unwrap_or_default();
            let error_text = error_text.map(CefString::to_string).unwrap_or_default();
            // No address change arrives for a failed load, so show the URL that failed.
            if let Some(handles) = lock(&self.shell).handles.get_mut(&self.id) {
                handles.pending_error = Some(error_page(&failed_url, &error_text));
                handles.committed_url = failed_url.clone();
                handles.showing_error = true;
            }
            update_tab(&self.shell, self.id, |tab| tab.page.url = failed_url);
        }

        // Filling in the failed load's own document, rather than navigating to an error
        // page, keeps the failed URL in the address bar and leaves history correct.
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: i32,
        ) {
            let Some(frame) = frame.filter(|frame| frame.is_main() != 0) else {
                return;
            };
            // A new tab's page takes focus when it finishes creating, so take it back.
            // The toolbar leaves the text alone if the address bar already has focus.
            let (is_blank_and_active, toolbar) = {
                let shell = lock(&self.shell);
                let tab = shell.strip.active().filter(|tab| tab.id == self.id);
                (
                    tab.is_some_and(|tab| tab.page.url.is_empty() || tab.page.url == BLANK_PAGE),
                    shell.toolbar.clone(),
                )
            };
            if is_blank_and_active && let Some(toolbar) = toolbar {
                let mut browser = toolbar.clone();
                if let Some(view) = browser_view_get_for_browser(Some(&mut browser)) {
                    view.request_focus();
                }
                send_to_toolbar(Some(&toolbar), &UiEvent::KeepAddressFocus);
            }
            let page = lock(&self.shell)
                .handles
                .get_mut(&self.id)
                .and_then(|handles| handles.pending_error.take());
            let Some(page) = page else {
                // A page that got a real response belongs in history. An error page's
                // finished load has no HTTP status.
                let shell = lock(&self.shell);
                let tab = shell
                    .strip
                    .get(self.id)
                    .filter(|tab| http_status_code > 0 && is_web_url(&tab.page.url));
                if let Some(tab) = tab {
                    shell.with_storage(|storage| storage.record_visit(&tab.page.url, &tab.title, now_ms()));
                }
                return;
            };
            let page = serde_json::to_string(&page).expect("Failed to serialize error page");
            let code = format!("document.open();document.write({page});document.close();");
            frame.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
        }
    }
}
