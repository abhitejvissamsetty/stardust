//! Callbacks for the browsers in the window: the toolbar and one page per tab.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use bridge::{
    Answer, DISCARD_CHECK_MS, DISCARD_IDLE_MS, DISCARD_LOADED_LIMIT, Decision, Download,
    DownloadList, DownloadState, ExtensionSource, InternalPage, KeyPress, LoadedTab, PageCommand,
    Permission, SavedWindows, SearchEngine, Session, SessionTab, Settings, Shortcut, SidePanelData,
    SitePermissions, StartupMode, SubmittedLogin, TabHistory, TabId, TabStrip, UiCommand, UiEvent,
    WindowAction, Workspaces, describe, error_page, is_risky_file, is_web_url, level_for_percent,
    login_origin, next_zoom_percent, normalize_homepage, percent_for_level, resolve_input,
    safe_file_name, shortcut_for, site_origin,
    storage_types::{FolderItems, SuggestionItem},
    tabs_to_discard, unique_path,
};
use cef::*;

use storage::{DownloadRecord, Storage};

use crate::{
    app::{open_window, open_window_in},
    data_dir, passwords,
    render::{
        CLIP_REQUEST_MESSAGE, CLIP_RESULT_MESSAGE, FILL_LOGIN_MESSAGE, LOGIN_SUBMITTED_MESSAGE,
        UI_COMMAND_MESSAGE,
    },
    ui::{
        MENU_FRAME_NAME, internal_page_for_url, internal_url, side_panel_url, sidebar_url,
        toolbar_url,
    },
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
/// The row of tabs across the top, which goes away when the tabs are down the side.
const TAB_ROW_HEIGHT: i32 = 40;
const SIDEBAR_WIDTH: i32 = 264;
const SIDEBAR_COLLAPSED_WIDTH: i32 = 56;
const SIDE_PANEL_WIDTH: i32 = 320;
const DOWNLOAD_BAR_HEIGHT: i32 = 44;
const FIND_BAR_HEIGHT: i32 = 44;
const BOOKMARKS_BAR_HEIGHT: i32 = 34;
const DIALOG_BAR_HEIGHT: i32 = 56;
/// Pages control this text, so a long message can't take over the bar.
const DIALOG_MESSAGE_LIMIT: usize = 500;
const SUGGESTION_LIMIT: usize = 6;
const BOOKMARKS_BAR_LIMIT: u32 = 200;
const SETTING_ASK_DOWNLOAD_LOCATION: &str = "ask_download_location";
const SETTING_SAVE_MEMORY: &str = "save_memory";
const SETTING_VERTICAL_TABS: &str = "vertical_tabs";
const SETTING_SIDEBAR_COLLAPSED: &str = "sidebar_collapsed";
const CLEAR_ORIGIN_LIMIT: u32 = 2000;
const SETTING_SITE_PERMISSIONS: &str = "site_permissions";
const SETTING_EXTENSIONS: &str = "extensions";
const SETTING_WORKSPACES: &str = "workspaces";
const SETTING_SIDE_PANEL: &str = "side_panel";
/// Each workspace's open tabs are saved under this, then its id.
const WORKSPACE_TABS_PREFIX: &str = "workspace_tabs_";
/// Extensions waiting for the person to agree to them. More than this drops the oldest.
const PENDING_EXTENSION_LIMIT: usize = 3;
const MEDIA_AUDIO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_AUDIO_CAPTURE as u32;
const MEDIA_VIDEO: u32 =
    sys::cef_media_access_permission_types_t::CEF_MEDIA_PERMISSION_DEVICE_VIDEO_CAPTURE as u32;
const PROMPT_CAMERA: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_CAMERA_STREAM as u32;
const PROMPT_MICROPHONE: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_MIC_STREAM as u32;
const PROMPT_LOCATION: u32 =
    sys::cef_permission_request_types_t::CEF_PERMISSION_TYPE_GEOLOCATION as u32;

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
    /// The browser's own Back and Forward list, for a tab restored with one.
    history: Option<TabHistory>,
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
    /// Identifies this window when a tab is dragged to another one.
    window_id: u32,
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
    bookmarks_bar_shown: bool,
    /// A page's waiting dialog: which tab asked, and how to answer it.
    dialog: Option<(TabId, PendingDialog)>,
    /// The browser for suggestions and bookmark folder menus, created when first needed.
    popup_view: Option<BrowserView>,
    popup: Option<Browser>,
    popup_overlay: Option<OverlayController>,
    /// Back and Forward lists for restored tabs waiting to be selected.
    restored_history: HashMap<TabId, TabHistory>,
    /// The person agreed to cancel active downloads by closing.
    close_confirmed: bool,
    find_text: String,
    find_match_case: bool,
    /// When each tab was last showing, for choosing which to unload.
    last_shown: HashMap<TabId, i64>,
    /// Tabs playing sound, which are never unloaded.
    audible: HashSet<TabId>,
    /// Tabs whose browser is closing to save memory. They stay in the strip.
    discarding: HashSet<TabId>,
    /// Tabs whose page asked before leaving when they were unloaded. They stay loaded.
    keep_loaded: HashSet<TabId>,
    /// Tabs from the command line after the first, to load once the window exists.
    background_loads: Vec<TabId>,
    /// A private window. Its pages use a profile that lives in memory, and nothing it
    /// does is saved: no history, session, downloads list or passwords.
    private: bool,
    /// Two tabs shown side by side, left then right. One of them is the active tab, the
    /// one the toolbar is about; the other is shown beside it.
    split: Option<(TabId, TabId)>,
    /// The box that holds the pages, and beside them the vertical tab list when it's on.
    body: Option<Panel>,
    /// The vertical tab list: its browser, the view that holds it, and whether its page
    /// has loaded and can be sent the tabs.
    sidebar: Option<Browser>,
    sidebar_view: Option<BrowserView>,
    sidebar_ready: bool,
    /// The side panel (notes, to-dos, snippets) at the right of the pages, when open.
    side_panel: Option<Browser>,
    side_panel_view: Option<BrowserView>,
    side_panel_ready: bool,
    /// The tab a clip has been asked for, until its page answers.
    pending_clip: Option<TabId>,
    /// The workspace this window belongs to. Empty is the main one, which uses the
    /// browser's original profile; any other has a profile of its own on disk.
    workspace: String,
    request_context: Option<RequestContext>,
    /// Page icons already fetched for this window, by address, and those being fetched.
    favicons: HashMap<String, String>,
    favicons_loading: HashSet<String>,
    /// Addresses and titles of tabs closed in this window, latest last.
    closed_tabs: Vec<(String, String)>,
    /// The extension popup showing under the toolbar, if any: whose it is, and what
    /// holds it. Only one shows at a time.
    ext_popup_id: Option<String>,
    ext_popup_view: Option<BrowserView>,
    ext_popup_overlay: Option<OverlayController>,
    ext_popup_browser: Option<Browser>,
}

const EXTENSION_POPUP_WIDTH: i32 = 380;
const EXTENSION_POPUP_HEIGHT: i32 = 520;

const CLOSED_TAB_LIMIT: usize = 20;
const FAVICON_LIMIT_BYTES: usize = 48 * 1024;

/// What the dialog bar is asking, and how to answer.
enum PendingDialog {
    /// An alert, confirm, prompt or "Leave site?" from a page.
    Page(JsdialogCallback),
    /// The camera or microphone, asked for by `getUserMedia`.
    Media {
        callback: MediaAccessCallback,
        requested: u32,
        site: String,
        permissions: Vec<Permission>,
    },
    /// Whether to save a login just sent from a page.
    SaveLogin {
        origin: String,
        username: String,
        password: String,
    },
    /// Other permissions, such as location.
    Prompt {
        callback: PermissionPromptCallback,
        id: u64,
        site: String,
        permissions: Vec<Permission>,
    },
}

/// How the person answered the dialog bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reply {
    Accept,
    Decline,
    /// Gone without an answer, such as by switching tabs. Nothing is remembered.
    Dismiss,
}

/// Data shared by every window.
struct SavedData {
    /// None if the database couldn't open. The browser still works, but saves nothing.
    storage: Option<Mutex<Storage>>,
    settings: Mutex<Settings>,
    /// The session as last saved, to avoid rewriting it when nothing changed.
    session: Mutex<String>,
    permissions: Mutex<SitePermissions>,
    extensions: Mutex<extensions::Registry>,
    workspaces: Mutex<Workspaces>,
    /// Each workspace's tabs as last saved, to avoid rewriting what didn't change.
    workspace_tabs: Mutex<HashMap<String, String>>,
    /// The side panel's notes, to-dos and snippets, shared by every window.
    panel: Mutex<SidePanelData>,
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
            ask_download_location: saved(SETTING_ASK_DOWNLOAD_LOCATION).as_deref() == Some("1"),
            save_memory: saved(SETTING_SAVE_MEMORY).as_deref() != Some("0"),
            vertical_tabs: saved(SETTING_VERTICAL_TABS).as_deref() == Some("1"),
            sidebar_collapsed: saved(SETTING_SIDEBAR_COLLAPSED).as_deref() == Some("1"),
        };
        let permissions = saved(SETTING_SITE_PERMISSIONS)
            .map(|json| SitePermissions::parse(&json))
            .unwrap_or_default();
        let extensions = saved(SETTING_EXTENSIONS)
            .map(|json| extensions::Registry::parse(&json))
            .unwrap_or_default();
        let workspaces = saved(SETTING_WORKSPACES)
            .map(|json| Workspaces::parse(&json))
            .unwrap_or_default();
        remove_forgotten_workspace_data(&workspaces);
        let panel = saved(SETTING_SIDE_PANEL)
            .map(|json| SidePanelData::parse(&json))
            .unwrap_or_default();
        SavedData {
            storage: storage.map(Mutex::new),
            settings: Mutex::new(settings),
            session: Mutex::default(),
            permissions: Mutex::new(permissions),
            extensions: Mutex::new(extensions),
            workspaces: Mutex::new(workspaces),
            workspace_tabs: Mutex::default(),
            panel: Mutex::new(panel),
        }
    })
}

fn extension_registry() -> MutexGuard<'static, extensions::Registry> {
    saved_data()
        .extensions
        .lock()
        .expect("Failed to lock the extensions list")
}

fn save_extension_registry() {
    let json = extension_registry().to_json();
    with_storage(|storage| storage.set_setting(SETTING_EXTENSIONS, &json));
}

fn extensions_dir() -> Option<PathBuf> {
    data_dir().map(|folder| folder.join("Extensions"))
}

/// Where extensions are unpacked while the person decides, beside the installed ones.
fn extensions_staging_dir() -> Option<PathBuf> {
    data_dir().map(|folder| folder.join("Extensions-staging"))
}

/// The ids of the extensions Chromium was asked to load when the browser started, to
/// tell the person when what they've changed needs a restart.
static EXTENSIONS_AT_START: OnceLock<BTreeSet<String>> = OnceLock::new();

fn enabled_extension_ids() -> BTreeSet<String> {
    extension_registry()
        .entries()
        .iter()
        .filter(|entry| entry.enabled)
        .map(|entry| entry.id.clone())
        .collect()
}

/// The folders of the enabled extensions, for Chromium to load. Called once, as the
/// browser starts. Chromium only loads extensions then, so changes wait for a restart.
pub fn extension_load_paths() -> Vec<PathBuf> {
    if let Some(staging) = extensions_staging_dir() {
        // Anything left from a package nobody finished adding.
        let _ = std::fs::remove_dir_all(staging);
    }
    let _ = EXTENSIONS_AT_START.set(enabled_extension_ids());
    extensions_dir()
        .map(|folder| extension_registry().load_paths(&folder))
        .unwrap_or_default()
}

fn extensions_event() -> serde_json::Value {
    let list = extensions_dir()
        .map(|folder| extension_registry().describe(&folder))
        .unwrap_or_default();
    let restart_needed = EXTENSIONS_AT_START
        .get()
        .is_some_and(|started| *started != enabled_extension_ids());
    serde_json::json!({ "type": "extensions", "extensions": list, "restartNeeded": restart_needed })
}

/// Packages unpacked and checked, waiting to be added or turned down.
static PENDING_EXTENSIONS: Mutex<Vec<(u64, extensions::Prepared)>> = Mutex::new(Vec::new());
static NEXT_EXTENSION_TOKEN: AtomicU64 = AtomicU64::new(1);

fn say(frame: &Frame, event: serde_json::Value) {
    let code = format!("window.onBrowserEvent({event})");
    frame.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
}

/// Unpacks what the person picked and shows them what it would be able to do.
fn prepare_extension(frame: &Frame, source: &Path) {
    let Some(staging_root) = extensions_staging_dir() else {
        return;
    };
    let token = NEXT_EXTENSION_TOKEN.fetch_add(1, Ordering::Relaxed);
    match extensions::prepare(source, &staging_root.join(token.to_string())) {
        Ok(prepared) => {
            let icon = prepared
                .summary
                .icon
                .as_deref()
                .and_then(|icon| extensions::icon_data_url(&prepared.root, icon));
            let event = serde_json::json!({
                "type": "extensionToInstall",
                "token": token,
                "extension": prepared.summary,
                "iconData": icon,
            });
            let mut pending = PENDING_EXTENSIONS
                .lock()
                .expect("Failed to lock pending extensions");
            pending.push((token, prepared));
            while pending.len() > PENDING_EXTENSION_LIMIT {
                let (_, old) = pending.remove(0);
                let _ = std::fs::remove_dir_all(old.staging);
            }
            drop(pending);
            say(frame, event);
        }
        Err(error) => say(
            frame,
            serde_json::json!({ "type": "extensionError", "message": error.to_string() }),
        ),
    }
}

fn take_pending_extension(token: u64) -> Option<extensions::Prepared> {
    let mut pending = PENDING_EXTENSIONS
        .lock()
        .expect("Failed to lock pending extensions");
    let index = pending
        .iter()
        .position(|(pending_token, _)| *pending_token == token)?;
    Some(pending.remove(index).1)
}

wrap_run_file_dialog_callback! {
    struct ExtensionPicked {
        frame: Frame,
    }

    impl RunFileDialogCallback {
        fn on_file_dialog_dismissed(&self, file_paths: Option<&mut CefStringList>) {
            let paths: Vec<String> = file_paths
                .map(std::mem::take)
                .map(|list| list.into_iter().collect())
                .unwrap_or_default();
            // Cancelling gives no paths.
            if let Some(path) = paths.first() {
                prepare_extension(&self.frame, Path::new(path));
            }
        }
    }
}

/// Shows the system's file picker for an extension, and carries on when one is chosen.
fn choose_extension(frame: &Frame, source: ExtensionSource) {
    // Tests can't drive the system's file picker, so in debug builds a path in the
    // environment stands in for it. The person still has to agree to the extension.
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("STARDUST_TEST_EXTENSION_PATH") {
        prepare_extension(frame, Path::new(&path));
        return;
    }
    let Some(host) = frame.browser().and_then(|browser| browser.host()) else {
        return;
    };
    let (mode, title) = match source {
        ExtensionSource::Folder => (FileDialogMode::OPEN_FOLDER, "Choose an extension's folder"),
        ExtensionSource::File => (FileDialogMode::OPEN, "Choose an extension file"),
    };
    let mut filters = CefStringList::new();
    if source == ExtensionSource::File {
        for filter in [".zip", ".crx"] {
            filters.append(filter);
        }
    }
    let mut callback = ExtensionPicked::new(frame.clone());
    host.run_file_dialog(
        mode,
        Some(&CefString::from(title)),
        None,
        Some(&mut filters),
        Some(&mut callback),
    );
}

fn site_permissions() -> MutexGuard<'static, SitePermissions> {
    saved_data()
        .permissions
        .lock()
        .expect("Failed to lock site permissions")
}

fn save_site_permissions() {
    let json = site_permissions().to_json();
    with_storage(|storage| storage.set_setting(SETTING_SITE_PERMISSIONS, &json));
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
    static NEXT_WINDOW_ID: AtomicU32 = AtomicU32::new(1);
    lock(shell).window_id = NEXT_WINDOW_ID.fetch_add(1, Ordering::Relaxed);
    WINDOWS
        .lock()
        .expect("Failed to lock windows")
        .push(shell.clone());
}

/// Closes every window, which ends the message loop.
pub fn close_all_browsers() {
    QUITTING.store(true, Ordering::Relaxed);
    if let Some(shell) = open_windows().into_iter().find(needs_close_confirmation) {
        ask_to_confirm_close(&shell);
        return;
    }
    for shell in open_windows() {
        close_window(&shell);
    }
}

fn needs_close_confirmation(shell: &SharedShell) -> bool {
    let shell = lock(shell);
    !shell.close_confirmed && shell.downloads.active_count() > 0
}

fn ask_to_confirm_close(shell: &SharedShell) {
    let (toolbar, downloads) = {
        let shell = lock(shell);
        (shell.toolbar.clone(), shell.downloads.active_count())
    };
    send_to_toolbar(toolbar.as_ref(), &UiEvent::ConfirmClose { downloads });
}

fn resolve_close_confirmation(shell: &SharedShell, close: bool) {
    if !close {
        QUITTING.store(false, Ordering::Relaxed);
        return;
    }
    if QUITTING.load(Ordering::Relaxed) {
        for window in open_windows() {
            lock(&window).close_confirmed = true;
        }
        close_all_browsers();
    } else {
        lock(shell).close_confirmed = true;
        close_window(shell);
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
    let (toolbar, strip, window_id, private, split) = {
        let shell = lock(shell);
        let mut strip = shell.strip.clone();
        // A tab with no browser yet, or one unloaded to save memory, shows dimmed.
        for id in shell.unloaded.keys() {
            if let Some(tab) = strip.get_mut(*id) {
                tab.discarded = true;
            }
        }
        (
            shell.toolbar.clone().filter(|_| shell.toolbar_ready),
            strip,
            shell.window_id,
            shell.private,
            shell.split,
        )
    };
    let event = UiEvent::Tabs {
        strip: &strip,
        window_id,
        private,
        split,
    };
    send_to_toolbar(toolbar.as_ref(), &event);
    send_to_toolbar(sidebar_of(shell).as_ref(), &event);
    save_session();
}

fn is_private(shell: &SharedShell) -> bool {
    lock(shell).private
}

/// Makes a window private. Call before its tabs are created.
pub fn make_private(shell: &SharedShell) {
    // No cache path means a profile that lives in memory and goes with the window.
    let settings = RequestContextSettings::default();
    let context = request_context_create_context(Some(&settings), None);
    let mut guard = lock(shell);
    guard.private = true;
    guard.request_context = context;
}

fn workspace_list() -> MutexGuard<'static, Workspaces> {
    saved_data()
        .workspaces
        .lock()
        .expect("Failed to lock the workspaces list")
}

fn save_workspace_list() {
    let json = workspace_list().to_json();
    with_storage(|storage| storage.set_setting(SETTING_WORKSPACES, &json));
}

/// A workspace's profile folder. CEF only accepts a profile that's a direct child of the
/// data folder, like `Default`; one nested deeper silently becomes a memory-only profile.
const WORKSPACE_FOLDER_PREFIX: &str = "Workspace-";

fn workspace_folder(id: &str) -> Option<PathBuf> {
    bridge::is_workspace_id(id)
        .then(|| data_dir().map(|folder| folder.join(format!("{WORKSPACE_FOLDER_PREFIX}{id}"))))
        .flatten()
}

/// Deletes the data of workspaces that were removed. Done at start-up, when nothing has
/// those folders open.
fn remove_forgotten_workspace_data(list: &Workspaces) {
    let Some(entries) = data_dir().and_then(|folder| std::fs::read_dir(folder).ok()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Only folders named exactly like a workspace's are ever touched.
        let Some(id) = name.strip_prefix(WORKSPACE_FOLDER_PREFIX) else {
            continue;
        };
        if bridge::is_workspace_id(id) && !list.items().iter().any(|item| item.id == id) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Gives a window the profile of a workspace: its own cookies, logins and site storage,
/// kept on disk. Call before its tabs are created, and make them only once `handler` says
/// the profile is ready. Returns false, changing nothing, for the main workspace or an
/// unknown one.
pub fn make_workspace(
    shell: &SharedShell,
    id: &str,
    handler: Option<&mut RequestContextHandler>,
) -> bool {
    if id.is_empty() || !workspace_list().exists(id) {
        return false;
    }
    let Some(path) = workspace_folder(id) else {
        return false;
    };
    let settings = RequestContextSettings {
        cache_path: CefString::from(path.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        ..Default::default()
    };
    let Some(context) = request_context_create_context(Some(&settings), handler) else {
        return false;
    };
    let mut guard = lock(shell);
    guard.workspace = id.to_owned();
    guard.request_context = Some(context);
    true
}

fn workspace_of(shell: &SharedShell) -> String {
    lock(shell).workspace.clone()
}

/// Every workspace with its colour, and whether a window of it is open.
fn workspace_items() -> Vec<serde_json::Value> {
    let saved: Vec<_> = workspace_list().items().to_vec();
    let open: HashSet<String> = open_windows()
        .iter()
        .filter_map(|shell| {
            let guard = lock(shell);
            (!guard.private && !guard.window_closing).then(|| guard.workspace.clone())
        })
        .collect();
    let mut items = vec![serde_json::json!({
        "id": "",
        "name": bridge::MAIN_NAME,
        "color": bridge::MAIN_COLOR,
        "main": true,
        "open": open.contains(""),
    })];
    items.extend(saved.iter().map(|item| {
        serde_json::json!({
            "id": item.id,
            "name": item.name,
            "color": item.color,
            "main": false,
            "open": open.contains(&item.id),
        })
    }));
    items
}

fn publish_workspaces(shell: &SharedShell) {
    let (toolbar, current) = {
        let guard = lock(shell);
        (
            guard.toolbar.clone().filter(|_| guard.toolbar_ready),
            guard.workspace.clone(),
        )
    };
    let items = workspace_items();
    let event = UiEvent::Workspaces {
        items: &items,
        current: &current,
    };
    send_to_toolbar(toolbar.as_ref(), &event);
    send_to_toolbar(sidebar_of(shell).as_ref(), &event);
}

fn publish_workspaces_everywhere() {
    for shell in open_windows() {
        publish_workspaces(&shell);
    }
}

/// Shows a workspace's window if one is open, and otherwise opens one with the tabs the
/// workspace had when it was last used.
fn open_workspace(id: &str) {
    if !workspace_list().exists(id) {
        return;
    }
    let existing = open_windows().into_iter().find(|shell| {
        let guard = lock(shell);
        !guard.private && !guard.window_closing && guard.workspace == id
    });
    if let Some(shell) = existing {
        let window = lock(&shell).window.clone();
        if let Some(window) = window {
            window.activate();
        }
        return;
    }
    let key = format!("{WORKSPACE_TABS_PREFIX}{id}");
    let saved = with_storage(|storage| storage.setting(&key))
        .flatten()
        .and_then(|json| SavedWindows::parse(&json).windows.into_iter().next())
        .filter(|session| !session.tabs.is_empty());
    match saved {
        Some(session) => open_window_in(Some(session), &[], false, id),
        None => open_window_in(None, &[BLANK_PAGE.to_owned()], false, id),
    }
}

/// The answer to a page asking for the workspaces, with an explanation if what it just
/// asked for wasn't allowed.
fn workspaces_event(error: Option<&str>) -> serde_json::Value {
    serde_json::json!({ "type": "workspaces", "items": workspace_items(), "error": error })
}

/// A title for the window's frame, which says when it's private or in another workspace.
fn window_title(shell: &SharedShell, title: &str) -> String {
    let (private, workspace) = {
        let guard = lock(shell);
        (guard.private, guard.workspace.clone())
    };
    if !private {
        if workspace.is_empty() {
            return title.to_owned();
        }
        let (name, _) = workspace_list().display(&workspace);
        return if title.is_empty() {
            name
        } else {
            format!("{title} \u{2014} {name}")
        };
    }
    if title.is_empty() {
        "Private window".to_owned()
    } else {
        format!("{title} \u{2014} Private")
    }
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
            // Private windows leave nothing behind, not even their tabs.
            (!shell.window_closing && !shell.private).then(|| Session {
                workspace: shell.workspace.clone(),
                ..Session::from_strip(&shell.strip)
            })
        })
        .collect();
    if windows.is_empty() {
        return;
    }
    save_workspace_tabs(&windows);
    let saved = SavedWindows { windows };
    let json = serde_json::to_string(&saved).expect("Failed to serialize session");
    let mut last = saved_data().session.lock().expect("Failed to lock session");
    if *last != json {
        with_storage(|storage| storage.set_setting(SETTING_SESSION, &json));
        *last = json;
    }
}

/// Keeps each workspace's tabs, so opening it later brings back where it was left. A
/// workspace's tabs aren't dropped when its window closes.
fn save_workspace_tabs(windows: &[Session]) {
    for session in windows
        .iter()
        .filter(|session| !session.workspace.is_empty())
    {
        let json = serde_json::to_string(&SavedWindows {
            windows: vec![session.clone()],
        })
        .expect("Failed to serialize workspace tabs");
        let mut last = saved_data()
            .workspace_tabs
            .lock()
            .expect("Failed to lock workspace tabs");
        if last.get(&session.workspace) != Some(&json) {
            let key = format!("{WORKSPACE_TABS_PREFIX}{}", session.workspace);
            with_storage(|storage| storage.set_setting(&key, &json));
            last.insert(session.workspace.clone(), json);
        }
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
    requested_urls: &[String],
) -> TabId {
    let mut guard = lock(shell);
    let session = session.unwrap_or_default();
    guard.bookmarks_bar_shown = has_bookmarks_for_bar();

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
        .map(|tab| {
            let id = add(&mut guard, &tab.url, &tab.title);
            if tab.entries.len() > 1 {
                let history = TabHistory::new(tab.entries.clone(), tab.index);
                if let Some(saved) = guard.strip.get_mut(id) {
                    saved.page.can_go_back = history.can_go_back();
                    saved.page.can_go_forward = history.can_go_forward();
                    saved.history_entries = history.entries.clone();
                    saved.history_index = history.index;
                }
                guard.restored_history.insert(id, history);
            }
            id
        })
        .collect();
    if let Some((first, rest)) = requested_urls.split_first() {
        let first = add(&mut guard, first, "");
        for url in rest {
            let id = add(&mut guard, url, "");
            guard.background_loads.push(id);
        }
        return first;
    }
    if let Some(id) = ids.get(session.active) {
        return *id;
    }
    let homepage = settings().homepage.clone();
    add(&mut guard, &homepage, "")
}

/// The toolbar view's height, which includes any bars that are showing.
pub fn toolbar_height(shell: &SharedShell) -> i32 {
    let vertical = settings().vertical_tabs;
    let shell = lock(shell);
    TOOLBAR_HEIGHT - if vertical { TAB_ROW_HEIGHT } else { 0 }
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
        + if shell.bookmarks_bar_shown {
            BOOKMARKS_BAR_HEIGHT
        } else {
            0
        }
        + if shell.dialog.is_some() {
            DIALOG_BAR_HEIGHT
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
    let match_case = {
        let mut guard = lock(shell);
        guard.find_text = text.to_owned();
        guard.find_match_case
    };
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
            match_case.into(),
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

fn has_bookmarks_for_bar() -> bool {
    with_storage(|storage| storage.bookmarks("", 1)).is_some_and(|bookmarks| !bookmarks.is_empty())
}

/// Sends the bookmarks bar to every window, resizing it if the bar appeared or went away.
fn publish_bookmarks_bar() {
    let bookmarks =
        with_storage(|storage| storage.bookmarks("", BOOKMARKS_BAR_LIMIT)).unwrap_or_default();
    let folders = with_storage(Storage::bookmark_folders).unwrap_or_default();
    let to_item = |bookmark: &storage::Bookmark| SuggestionItem {
        url: bookmark.url.clone(),
        title: bookmark.title.clone(),
        is_bookmark: false,
    };
    let loose: Vec<SuggestionItem> = bookmarks
        .iter()
        .filter(|bookmark| bookmark.folder_id.is_none())
        .map(to_item)
        .collect();
    let filed: Vec<FolderItems> = folders
        .iter()
        .map(|folder| FolderItems {
            name: folder.name.clone(),
            items: bookmarks
                .iter()
                .filter(|bookmark| bookmark.folder_id == Some(folder.id))
                .map(to_item)
                .collect(),
        })
        .collect();
    let show_bar = !bookmarks.is_empty() || !folders.is_empty();
    for shell in open_windows() {
        let (toolbar, resized) = {
            let mut shell = lock(&shell);
            let resized = shell.bookmarks_bar_shown != show_bar;
            shell.bookmarks_bar_shown = show_bar;
            (
                shell.toolbar.clone().filter(|_| shell.toolbar_ready),
                resized,
            )
        };
        send_to_toolbar(
            toolbar.as_ref(),
            &UiEvent::BookmarksBar {
                bookmarks: &loose,
                folders: &filed,
            },
        );
        if resized {
            resize_toolbar(&shell);
        }
    }
}

fn suggest(shell: &SharedShell, query: &str) {
    let items: Vec<SuggestionItem> =
        with_storage(|storage| storage.suggestions(query, SUGGESTION_LIMIT))
            .unwrap_or_default()
            .into_iter()
            .map(|item| SuggestionItem {
                url: item.url,
                title: item.title,
                is_bookmark: item.is_bookmark,
            })
            .collect();
    let toolbar = lock(shell).toolbar.clone();
    send_to_toolbar(
        toolbar.as_ref(),
        &UiEvent::Suggestions {
            query,
            items: &items,
        },
    );
}

fn send_to_popup(popup: Option<&Browser>, contents: &serde_json::Value) {
    if let Some(frame) = popup.and_then(|popup| popup.main_frame()) {
        let code = format!("window.onBrowserEvent({contents})");
        frame.execute_java_script(Some(&CefString::from(code.as_str())), None, 0);
    }
}

fn ensure_popup_view(shell: &SharedShell) -> Option<OverlayController> {
    lock(shell).popup_overlay.clone()
}

/// Places the menu the toolbar opened as a hidden overlay under the toolbar. Returns
/// whether it was placed.
pub fn adopt_menu_view(shell: &SharedShell, view: &BrowserView) -> bool {
    let window = {
        let shell = lock(shell);
        if shell.popup_view.is_some() {
            return false;
        }
        shell.window.clone()
    };
    let Some(window) = window else {
        return false;
    };
    // Typing stays in the address bar while the menu is showing.
    View::from(view).set_focusable(0);
    let Some(overlay) =
        window.add_overlay_view(Some(&mut View::from(view)), DockingMode::CUSTOM, 0)
    else {
        return false;
    };
    overlay.set_visible(0);
    let mut guard = lock(shell);
    guard.popup_view = Some(view.clone());
    guard.popup_overlay = Some(overlay);
    true
}

fn show_popup(
    shell: &SharedShell,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    contents: serde_json::Value,
) {
    let Some(overlay) = ensure_popup_view(shell) else {
        return;
    };
    overlay.set_bounds(Some(&Rect {
        x: left,
        y: top,
        width,
        height,
    }));
    overlay.set_visible(1);
    let popup = lock(shell).popup.clone();
    send_to_popup(popup.as_ref(), &contents);
}

fn focus_active_tab(shell: &SharedShell) {
    let (url, view) = {
        let shell = lock(shell);
        let tab = shell.strip.active();
        let view = tab
            .and_then(|tab| shell.handles.get(&tab.id))
            .map(|handles| handles.view.clone());
        (
            tab.map(|tab| tab.page.url.clone()).unwrap_or_default(),
            view,
        )
    };
    if url.is_empty() || url == BLANK_PAGE {
        focus_address(shell);
    } else if let Some(view) = view {
        view.request_focus();
    }
}

/// The window became active, perhaps after a page's dialog closed. That leaves keyboard
/// focus nowhere until something takes it, so shortcuts would stop working.
pub fn window_activated(shell: &SharedShell) {
    let toolbar = lock(shell).toolbar.clone();
    if let Some(toolbar) = toolbar {
        focus_toolbar(&toolbar);
    }
    focus_active_tab(shell);
}

/// Shows a page's dialog in the toolbar. Only the showing tab may ask, and only one
/// question at a time. Returns false if this one can't be shown.
fn show_dialog(
    shell: &SharedShell,
    id: TabId,
    kind: &str,
    origin: &str,
    message: &str,
    default_text: &str,
    pending: PendingDialog,
) -> bool {
    let toolbar = {
        let mut guard = lock(shell);
        if guard.dialog.is_some() || guard.strip.active_id() != Some(id) {
            return false;
        }
        guard.dialog = Some((id, pending));
        guard.toolbar.clone()
    };
    resize_toolbar(shell);
    let message: String = message.chars().take(DIALOG_MESSAGE_LIMIT).collect();
    if let Some(toolbar) = toolbar {
        focus_toolbar(&toolbar);
        send_to_toolbar(
            Some(&toolbar),
            &UiEvent::ShowDialog {
                kind,
                origin,
                message: &message,
                default_text,
            },
        );
    }
    true
}

fn answer_dialog(shell: &SharedShell, reply: Reply, input: &str) {
    let Some((_, pending)) = lock(shell).dialog.take() else {
        return;
    };
    close_dialog_bar(shell);
    let accepted = reply == Reply::Accept;
    match pending {
        PendingDialog::Page(callback) => {
            callback.cont(accepted.into(), Some(&CefString::from(input)));
            // Staying on a page after "Leave site?" also calls off the window closing.
            if !accepted {
                lock(shell).window_closing = false;
                QUITTING.store(false, Ordering::Relaxed);
            }
        }
        PendingDialog::Media {
            callback,
            requested,
            site,
            permissions,
        } => {
            remember_permission(reply, &site, &permissions);
            if accepted {
                callback.cont(requested);
            } else {
                callback.cancel();
            }
        }
        PendingDialog::SaveLogin {
            origin,
            username,
            password,
        } => match reply {
            Reply::Accept => {
                if passwords::save(&origin, &username, &password).is_ok() {
                    with_storage(|storage| storage.save_login(&origin, &username, now_ms()));
                    refresh_login_keys();
                }
            }
            Reply::Decline => {
                with_storage(|storage| storage.set_never_save_login(&origin, true));
            }
            Reply::Dismiss => {}
        },
        PendingDialog::Prompt {
            callback,
            site,
            permissions,
            ..
        } => {
            remember_permission(reply, &site, &permissions);
            callback.cont(match reply {
                Reply::Accept => PermissionRequestResult::ACCEPT,
                Reply::Decline => PermissionRequestResult::DENY,
                Reply::Dismiss => PermissionRequestResult::DISMISS,
            });
        }
    }
    focus_active_tab(shell);
}

/// The key a login is saved under. The main workspace uses the site's own origin, as
/// logins always have. Another workspace puts its id in front, so its accounts stay
/// apart from every other workspace's, even on the same site. `|` can't be in an origin.
fn login_key(workspace: &str, origin: &str) -> String {
    if workspace.is_empty() {
        origin.to_owned()
    } else {
        format!("{workspace}|{origin}")
    }
}

/// Splits a saved key back into its workspace and site.
fn split_login_key(key: &str) -> (&str, &str) {
    match key.split_once('|') {
        Some((workspace, origin)) if bridge::is_workspace_id(workspace) => (workspace, origin),
        _ => ("", key),
    }
}

/// The saved logins and never-save sites, for Settings, each with its workspace's name.
fn logins_event() -> serde_json::Value {
    let describe = |key: &str| {
        let (workspace, site) = split_login_key(key);
        let name = if workspace.is_empty() {
            String::new()
        } else {
            workspace_list().display(workspace).0
        };
        (site.to_owned(), name)
    };
    let logins: Vec<serde_json::Value> = with_storage(|storage| storage.logins(None))
        .unwrap_or_default()
        .iter()
        .map(|login| {
            let (site, workspace) = describe(&login.origin);
            serde_json::json!({
                "origin": login.origin,
                "site": site,
                "workspace": workspace,
                "username": login.username,
            })
        })
        .collect();
    let never: Vec<serde_json::Value> = with_storage(Storage::never_save_login_sites)
        .unwrap_or_default()
        .iter()
        .map(|key| {
            let (site, workspace) = describe(key);
            serde_json::json!({ "origin": key, "site": site, "workspace": workspace })
        })
        .collect();
    serde_json::json!({ "type": "logins", "logins": logins, "neverSave": never })
}

/// Whether a page's site has saved logins in this workspace, for the toolbar's key button.
fn has_saved_logins(workspace: &str, url: &str) -> bool {
    login_origin(url).is_some_and(|origin| {
        let key = login_key(workspace, &origin);
        with_storage(|storage| storage.logins(Some(&key))).is_some_and(|logins| !logins.is_empty())
    })
}

/// Updates every tab's key button after logins change.
fn refresh_login_keys() {
    for shell in open_windows() {
        let changed = {
            let mut guard = lock(&shell);
            let mut changed = false;
            let workspace = guard.workspace.clone();
            let ids: Vec<TabId> = guard.strip.tabs().iter().map(|tab| tab.id).collect();
            for id in ids {
                if let Some(tab) = guard.strip.get_mut(id) {
                    let has_logins = has_saved_logins(&workspace, &tab.page.url);
                    changed |= tab.page.has_logins != has_logins;
                    tab.page.has_logins = has_logins;
                }
            }
            changed
        };
        if changed {
            publish(&shell);
        }
    }
}

/// A page reported a login being sent. Offers to save it, unless it's saved already
/// or the person said never for this site.
fn offer_to_save_login(shell: &SharedShell, tab: TabId, url: &str, json: &str) {
    // Private windows don't offer to save passwords.
    if is_private(shell) {
        return;
    }
    let (Some(site), Some(login)) = (login_origin(url), SubmittedLogin::parse(json)) else {
        return;
    };
    // Saved under this workspace's own key, so accounts don't mix between workspaces.
    let origin = login_key(&workspace_of(shell), &site);
    if with_storage(|storage| storage.never_saves_logins(&origin)).unwrap_or(true) {
        return;
    }
    let saved = passwords::get(&origin, &login.username);
    if saved.as_deref() == Some(login.password.as_str()) {
        with_storage(|storage| storage.save_login(&origin, &login.username, now_ms()));
        return;
    }
    let who = if login.username.is_empty() {
        "this site".to_owned()
    } else {
        login.username.clone()
    };
    let message = if saved.is_some() {
        format!("Update the saved password for {who}?")
    } else {
        format!("Save the password for {who}?")
    };
    let host = site
        .split_once("://")
        .map_or(site.as_str(), |(_, host)| host)
        .to_owned();
    let pending = PendingDialog::SaveLogin {
        origin,
        username: login.username,
        password: login.password,
    };
    show_dialog(shell, tab, "savePassword", &host, &message, "", pending);
}

/// Fills the active page's most recently used login, when the person asks.
fn fill_login(shell: &SharedShell) {
    let Some(browser) = active_browser(shell) else {
        return;
    };
    let Some(frame) = browser.main_frame() else {
        return;
    };
    let Some(site) = login_origin(&CefString::from(&frame.url()).to_string()) else {
        return;
    };
    // Only this workspace's logins, so another account on the site is never offered.
    let key = login_key(&workspace_of(shell), &site);
    let Some(login) = with_storage(|storage| storage.logins(Some(&key)))
        .and_then(|logins| logins.into_iter().next())
    else {
        return;
    };
    let Some(password) = passwords::get(&key, &login.username) else {
        return;
    };
    let Some(mut message) = process_message_create(Some(&CefString::from(FILL_LOGIN_MESSAGE)))
    else {
        return;
    };
    if let Some(list) = message.argument_list() {
        list.set_string(0, Some(&CefString::from(site.as_str())));
        list.set_string(1, Some(&CefString::from(login.username.as_str())));
        list.set_string(2, Some(&CefString::from(password.as_str())));
    }
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    with_storage(|storage| storage.save_login(&key, &login.username, now_ms()));
}

/// Tells the toolbar how its window is showing.
fn publish_window_state(shell: &SharedShell) {
    let (window, toolbar) = {
        let shell = lock(shell);
        (shell.window.clone(), shell.toolbar.clone())
    };
    let Some(window) = window else {
        return;
    };
    send_to_toolbar(
        toolbar.as_ref(),
        &UiEvent::WindowState {
            maximized: window.is_maximized() != 0,
            minimized: window.is_minimized() != 0,
            fullscreen: window.is_fullscreen() != 0,
        },
    );
}

/// What Chrome's title bar and window menu do.
fn window_action(shell: &SharedShell, action: WindowAction) {
    let Some(window) = lock(shell).window.clone() else {
        return;
    };
    match action {
        WindowAction::Minimize => window.minimize(),
        WindowAction::ToggleMaximize => {
            if window.is_fullscreen() != 0 {
                window.set_fullscreen(0);
            } else if window.is_maximized() != 0 {
                window.restore();
            } else {
                window.maximize();
            }
        }
        WindowAction::ToggleFullscreen => {
            window.set_fullscreen((window.is_fullscreen() == 0).into())
        }
    }
    publish_window_state(shell);
}

fn remember_permission(reply: Reply, site: &str, permissions: &[Permission]) {
    let decision = match reply {
        Reply::Accept => Decision::Allow,
        Reply::Decline => Decision::Block,
        Reply::Dismiss => return,
    };
    site_permissions().remember(site, permissions, decision);
    save_site_permissions();
}

/// Decides a site's request from saved choices, or asks in the dialog bar. Returns
/// whether it's allowed now, blocked now, or waiting for the person.
fn ask_permission(
    shell: &SharedShell,
    tab: TabId,
    requesting_origin: &str,
    permissions: Vec<Permission>,
    make_pending: impl FnOnce(String, Vec<Permission>) -> PendingDialog,
) -> Option<bool> {
    let Some(site) = site_origin(requesting_origin) else {
        return Some(false);
    };
    match site_permissions().answer(&site, &permissions) {
        Answer::Allow => return Some(true),
        Answer::Block => return Some(false),
        Answer::Ask(_) => {}
    }
    let host = site
        .split_once("://")
        .map_or(site.as_str(), |(_, host)| host)
        .to_owned();
    let message = format!("wants to {}", describe(&permissions));
    let pending = make_pending(site, permissions);
    // A tab out of view, or one already asking something, is refused for now.
    if show_dialog(shell, tab, "permission", &host, &message, "", pending) {
        None
    } else {
        Some(false)
    }
}

fn close_dialog_bar(shell: &SharedShell) {
    let toolbar = lock(shell).toolbar.clone();
    send_to_toolbar(toolbar.as_ref(), &UiEvent::HideDialog);
    resize_toolbar(shell);
}

fn hide_popup(shell: &SharedShell) {
    let overlay = lock(shell).popup_overlay.clone();
    if let Some(overlay) = overlay {
        overlay.set_visible(0);
    }
}

/// Runs a tab's Back or Forward, from its own list if it was restored with one.
fn go_in_history(shell: &SharedShell, forward: bool) {
    let target = {
        let mut guard = lock(shell);
        let id = guard.strip.active_id();
        let handles = id.and_then(|id| guard.handles.get_mut(&id));
        handles
            .and_then(|handles| handles.history.as_mut())
            .map(|history| {
                let url = if forward {
                    history.go_forward()
                } else {
                    history.go_back()
                };
                (
                    url,
                    history.can_go_back(),
                    history.can_go_forward(),
                    history.entries.clone(),
                    history.index,
                )
            })
    };
    let Some(page) = active_browser(shell) else {
        return;
    };
    match target {
        None if forward => page.go_forward(),
        None => page.go_back(),
        Some((url, can_back, can_forward, entries, index)) => {
            let id = lock(shell).strip.active_id();
            if let Some(id) = id {
                update_tab(shell, id, |tab| {
                    tab.page.can_go_back = can_back;
                    tab.page.can_go_forward = can_forward;
                    tab.history_entries = entries;
                    tab.history_index = index;
                });
            }
            if let (Some(url), Some(frame)) = (url, page.main_frame()) {
                let url = InternalPage::from_address(&url)
                    .map_or(url.clone(), |page| internal_url(page).to_owned());
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
    }
}

wrap_navigation_entry_visitor! {
    struct CollectEntries {
        entries: Arc<Mutex<(Vec<String>, usize)>>,
    }

    impl NavigationEntryVisitor {
        fn visit(
            &self,
            entry: Option<&mut NavigationEntry>,
            current: i32,
            _index: i32,
            _total: i32,
        ) -> i32 {
            if let Some(entry) = entry {
                let url = display_url(&CefString::from(&entry.url()).to_string());
                let mut saved = self.entries.lock().expect("Failed to lock entries");
                if current != 0 {
                    saved.1 = saved.0.len();
                }
                saved.0.push(url);
            }
            1
        }
    }
}

/// Copies a tab's Back and Forward list, so it can be saved with the session.
fn capture_history(shell: &SharedShell, id: TabId, browser: &Browser) {
    let (own_list, _) = {
        let guard = lock(shell);
        let handles = guard.handles.get(&id);
        (handles.and_then(|handles| handles.history.clone()), ())
    };
    if let Some(history) = own_list {
        update_tab(shell, id, |tab| {
            tab.history_entries = history.entries.clone();
            tab.history_index = history.index;
            tab.page.can_go_back = history.can_go_back();
            tab.page.can_go_forward = history.can_go_forward();
        });
        return;
    }
    let collected = Arc::new(Mutex::new((Vec::new(), 0)));
    if let Some(host) = browser.host() {
        let mut visitor = CollectEntries::new(collected.clone());
        host.navigation_entries(Some(&mut visitor), 0);
    }
    let (entries, index) = collected.lock().expect("Failed to lock entries").clone();
    update_tab(shell, id, |tab| {
        tab.history_entries = entries;
        tab.history_index = index;
    });
}

/// Moves a tab to another window, or a new one if no window is given. Its page reloads
/// there, because a client belongs to one window.
fn move_tab(source: &SharedShell, id: TabId, target: Option<SharedShell>) {
    let (moved, tab_count) = {
        let guard = lock(source);
        (guard.strip.get(id).cloned(), guard.strip.tabs().len())
    };
    let Some(moved) = moved else {
        return;
    };
    // A window's only tab would just become an identical new window.
    let is_blank = moved.page.url.is_empty() || moved.page.url == BLANK_PAGE;
    if is_blank || (target.is_none() && tab_count <= 1) {
        return;
    }
    // A tab can't cross between private and ordinary windows, which would put what a
    // private window keeps in memory into one that saves it, or the other way round.
    if target.as_ref().is_some_and(|target| {
        is_private(target) != is_private(source) || workspace_of(target) != workspace_of(source)
    }) {
        return;
    }
    let saved = SessionTab {
        url: moved.page.url.clone(),
        title: moved.title.clone(),
        entries: moved.history_entries.clone(),
        index: moved.history_index,
    };
    match target {
        Some(target) => {
            let new_id = add_restored_tab(&target, &saved);
            select_tab(&target, new_id);
            let window = lock(&target).window.clone();
            if let Some(window) = window {
                window.activate();
            }
        }
        None => open_window(
            Some(Session {
                tabs: vec![saved],
                active: 0,
                workspace: workspace_of(source),
            }),
            &[],
            is_private(source),
        ),
    }
    close_tab(source, id);
}

fn add_restored_tab(shell: &SharedShell, saved: &SessionTab) -> TabId {
    let mut guard = lock(shell);
    let id = guard.strip.add();
    let is_bookmarked = with_storage(|storage| storage.is_bookmarked(&saved.url)).unwrap_or(false);
    let history =
        (saved.entries.len() > 1).then(|| TabHistory::new(saved.entries.clone(), saved.index));
    if let Some(tab) = guard.strip.get_mut(id) {
        tab.page.url = saved.url.clone();
        tab.page.is_bookmarked = is_bookmarked;
        tab.title = saved.title.clone();
        if let Some(history) = &history {
            tab.page.can_go_back = history.can_go_back();
            tab.page.can_go_forward = history.can_go_forward();
            tab.history_entries = history.entries.clone();
            tab.history_index = history.index;
        }
    }
    if let Some(history) = history {
        guard.restored_history.insert(id, history);
    }
    guard.unloaded.insert(id, saved.url.clone());
    id
}

/// Opens a downloaded file with its usual app. Risky types only open after the person
/// has confirmed, and this never runs a file directly.
fn open_download_file(path: &Path, confirmed: bool) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !path.is_file() || (is_risky_file(&name) && !confirmed) {
        return false;
    }
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };
    match command.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            true
        }
        Err(_) => false,
    }
}

fn state_name(state: DownloadState) -> &'static str {
    match state {
        DownloadState::Complete => "complete",
        DownloadState::Cancelled => "cancelled",
        DownloadState::Failed => "failed",
        DownloadState::InProgress => "inProgress",
    }
}

fn saved_download_json(record: &DownloadRecord) -> serde_json::Value {
    serde_json::json!({
        "path": record.path,
        "fileName": record.file_name,
        "state": record.state,
        "receivedBytes": record.received_bytes,
        "finishedMs": record.finished_ms,
        "isRisky": is_risky_file(&record.file_name),
        "exists": Path::new(&record.path).is_file(),
    })
}

fn is_saved_download(path: &str) -> bool {
    with_storage(Storage::downloads)
        .unwrap_or_default()
        .iter()
        .any(|record| record.path == path)
}

/// Erases cookies and other data websites keep in the browser, then the cache.
fn clear_site_data(shell: &SharedShell, cookies: bool, cache: bool) {
    let Some(host) = active_browser(shell).and_then(|browser| browser.host()) else {
        return;
    };
    if cookies {
        if let Some(manager) = cookie_manager_get_global_manager(None) {
            manager.delete_cookies(None, None, None);
        }
        // Local storage and similar data can only be erased per site, so this covers
        // the sites in history.
        let origins: std::collections::BTreeSet<String> =
            with_storage(|storage| storage.history("", CLEAR_ORIGIN_LIMIT))
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| web_origin(&entry.url))
                .collect();
        for origin in origins {
            if let Some(mut params) = dictionary_value_create() {
                params.set_string(
                    Some(&CefString::from("origin")),
                    Some(&CefString::from(origin.as_str())),
                );
                params.set_string(
                    Some(&CefString::from("storageTypes")),
                    Some(&CefString::from("all")),
                );
                host.execute_dev_tools_method(
                    0,
                    Some(&CefString::from("Storage.clearDataForOrigin")),
                    Some(&mut params),
                );
            }
        }
    }
    if cache {
        host.execute_dev_tools_method(0, Some(&CefString::from("Network.clearBrowserCache")), None);
    }
}

fn web_origin(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    (!host.is_empty()).then(|| format!("{scheme}://{host}"))
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
    let mut context = lock(shell).request_context.clone();
    let view = browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        context.as_mut(),
        Some(&mut PageViewHost::new(shell.clone())),
    )
    .expect("Failed to create browser view");
    store_view(shell, id, view);
}

fn store_view(shell: &SharedShell, id: TabId, view: BrowserView) {
    let mut guard = lock(shell);
    let history = guard.restored_history.remove(&id);
    guard.last_shown.insert(id, now_ms());
    guard.handles.insert(
        id,
        TabHandles {
            view,
            browser: None,
            pending_error: None,
            committed_url: String::new(),
            showing_error: false,
            history,
        },
    );
}

/// Adds a tab's view to the window and shows it. Called once the window exists.
pub fn attach_tab(shell: &SharedShell, id: TabId) {
    let (body, layout, view) = {
        let shell = lock(shell);
        (
            shell.body.clone(),
            shell.layout.clone(),
            shell.handles.get(&id).map(|handles| handles.view.clone()),
        )
    };
    if let (Some(body), Some(view)) = (body, view) {
        let mut view = View::from(&view);
        body.add_child_view(Some(&mut view));
        if let Some(layout) = layout {
            layout.set_flex_for_view(Some(&mut view), 1);
        }
    }
    select_tab(shell, id);
}

/// `body` holds the pages, with `layout` as its layout.
pub fn set_window(shell: &SharedShell, window: Window, body: Panel, layout: Option<BoxLayout>) {
    let mut shell = lock(shell);
    shell.window = Some(window);
    shell.body = Some(body);
    shell.layout = layout;
}

/// Shows a tab, hides the others and moves focus to it.
pub fn select_tab(shell: &SharedShell, id: TabId) {
    hide_extension_popup(shell);
    let has_other_dialog = lock(shell)
        .dialog
        .as_ref()
        .is_some_and(|(tab, _)| *tab != id);
    if has_other_dialog {
        answer_dialog(shell, Reply::Dismiss, "");
    }
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

    let shown_beside;
    let (views, window, title, url) = {
        let mut shell = lock(shell);
        let previous = shell.strip.active_id();
        if !shell.strip.select(id) {
            return;
        }
        // In split view, a tab picked from outside the two replaces the one in use.
        if let Some((left, right)) = shell.split
            && id != left
            && id != right
        {
            shell.split = if previous == Some(left) {
                Some((id, right))
            } else {
                Some((left, id))
            };
        }
        let now = now_ms();
        if let Some(previous) = previous {
            shell.last_shown.insert(previous, now);
        }
        shell.last_shown.insert(id, now);
        let views: Vec<(TabId, BrowserView)> = shell
            .handles
            .iter()
            .map(|(id, handles)| (*id, handles.view.clone()))
            .collect();
        let tab = shell.strip.get(id);
        shown_beside = shell.split;
        (
            views,
            shell.window.clone(),
            tab.map(|tab| tab.title.clone()).unwrap_or_default(),
            tab.map(|tab| tab.page.url.clone()).unwrap_or_default(),
        )
    };
    for (tab_id, view) in &views {
        let shown = *tab_id == id
            || shown_beside.is_some_and(|(left, right)| *tab_id == left || *tab_id == right);
        View::from(view).set_visible(shown.into());
    }
    order_split_panes(shell);
    if let Some(window) = window {
        window.layout();
        let title = window_title(shell, &title);
        window.set_title(Some(&CefString::from(title.as_str())));
    }
    if url.is_empty() || url == BLANK_PAGE {
        focus_address(shell);
    } else if let Some((_, view)) = views.iter().find(|(tab_id, _)| *tab_id == id) {
        view.request_focus();
    }
    publish(shell);
}

/// Gives the address bar the keyboard, ready to take a command-bar search.
fn open_command_bar(shell: &SharedShell) {
    let toolbar = lock(shell).toolbar.clone();
    if let Some(toolbar) = toolbar {
        let mut browser = toolbar.clone();
        if let Some(view) = browser_view_get_for_browser(Some(&mut browser)) {
            view.request_focus();
        }
        send_to_toolbar(Some(&toolbar), &UiEvent::CommandBar);
    }
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
        let closed = shell
            .strip
            .get(id)
            .filter(|tab| is_web_url(&tab.page.url) && !shell.private)
            .map(|tab| (tab.page.url.clone(), tab.title.clone()));
        if let Some(closed) = closed {
            shell.closed_tabs.push(closed);
            if shell.closed_tabs.len() > CLOSED_TAB_LIMIT {
                shell.closed_tabs.remove(0);
            }
        }
        shell.strip.remove(id);
        shell.last_shown.remove(&id);
        shell.audible.remove(&id);
        shell.discarding.remove(&id);
        shell.keep_loaded.remove(&id);
        // Closing one of two tabs shown side by side ends the split and keeps the other.
        let mut next = shell.strip.active_id();
        if let Some((left, right)) = shell.split
            && (left == id || right == id)
        {
            shell.split = None;
            next = Some(if left == id { right } else { left });
        }
        (shell.body.clone(), view, next)
    };
    if let (Some(body), Some(view)) = (window, view) {
        body.remove_child_view(Some(&mut View::from(&view)));
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
            publish_bookmarks_bar();
            publish_extensions(shell);
            publish_workspaces(shell);
            publish_layout(shell);
            publish_side_panel_state(shell);
            send_known_favicons(shell);
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
            match_case,
        } => {
            lock(shell).find_match_case = match_case;
            run_find(shell, &text, forward, again);
        }
        UiCommand::CloseFind => close_find(shell, true),
        UiCommand::Suggest { query } => suggest(shell, &query),
        UiCommand::ShowPopup {
            left,
            top,
            width,
            height,
            contents,
        } => {
            show_popup(shell, left, top, width, height, contents);
        }
        UiCommand::HidePopup => {
            hide_popup(shell);
            hide_extension_popup(shell);
        }
        UiCommand::PopupReady => {
            // The menu's browser takes focus when it starts, and the toolbar opened
            // it while someone was using the toolbar, so focus goes back there.
            let toolbar = lock(shell).toolbar.clone();
            if let Some(toolbar) = toolbar {
                focus_toolbar(&toolbar);
                send_to_toolbar(Some(&toolbar), &UiEvent::MenuReady);
            }
        }
        UiCommand::ConfirmClose { close } => resolve_close_confirmation(shell, close),
        UiCommand::AnswerDialog { accepted, input } => {
            let reply = if accepted {
                Reply::Accept
            } else {
                Reply::Decline
            };
            answer_dialog(shell, reply, &input);
        }
        UiCommand::TearOffTab { id } => move_tab(shell, id, None),
        UiCommand::AdoptTab { from_window, id } => {
            let source = open_windows().into_iter().find(|window| {
                lock(window).window_id == from_window && !Arc::ptr_eq(window, shell)
            });
            if let Some(source) = source {
                move_tab(&source, id, Some(shell.clone()));
            }
        }
        UiCommand::OpenDownload { id, confirmed } => {
            let path = {
                let guard = lock(shell);
                guard
                    .downloads
                    .get(id)
                    .filter(|item| item.state == DownloadState::Complete)
                    .map(|item| item.path.clone())
            };
            if let Some(path) = path {
                open_download_file(&path, confirmed);
            }
        }
        UiCommand::OpenPage { page } => open_page(shell, page),
        UiCommand::FillLogin => fill_login(shell),
        UiCommand::WindowAction { action } => window_action(shell, action),
        UiCommand::GetWindowState => publish_window_state(shell),
        UiCommand::NewWindow => open_window_in(
            None,
            &[BLANK_PAGE.to_owned()],
            is_private(shell),
            &workspace_of(shell),
        ),
        UiCommand::NewPrivateWindow => open_window(None, &[BLANK_PAGE.to_owned()], true),
        UiCommand::OpenWorkspace { id } => open_workspace(&id),
        UiCommand::ToggleSplitView => toggle_split_view(shell),
        UiCommand::ToggleSidePanel => toggle_side_panel(shell),
        UiCommand::CommandBar => open_command_bar(shell),
        // Only the side panel sends this, and it's handled where it arrives.
        UiCommand::SidePanelReady => {}
        UiCommand::SetNotes { text } => change_panel_data(|data| {
            data.set_notes(&text);
            true
        }),
        UiCommand::AddTodo { text } => change_panel_data(|data| data.add_todo(&text)),
        UiCommand::ToggleTodo { id } => change_panel_data(|data| data.toggle_todo(id)),
        UiCommand::RemoveTodo { id } => change_panel_data(|data| data.remove_todo(id)),
        UiCommand::ClearDoneTodos => change_panel_data(SidePanelData::clear_done),
        UiCommand::AddSnippet { title, text } => {
            change_panel_data(|data| data.add_snippet(&title, &text))
        }
        UiCommand::RemoveSnippet { id } => change_panel_data(|data| data.remove_snippet(id)),
        UiCommand::AddCurrentPageSnippet => add_current_page_snippet(shell),
        UiCommand::ClipPage => clip_page(shell),
        UiCommand::RemoveClip { id } => change_panel_data(|data| data.remove_clip(id)),
        UiCommand::OpenClip { id } => {
            let url = panel_data().clip_url(id).map(str::to_owned);
            if let Some(url) = url.filter(|url| is_web_url(url)) {
                run_command(shell, UiCommand::Navigate { input: url });
            }
        }
        // Only the tab list sends this, and it's handled where it arrives.
        UiCommand::SidebarReady => {}
        UiCommand::SetVerticalTabs { enabled } => set_vertical_tabs(enabled),
        UiCommand::SetSidebarCollapsed { collapsed } => set_sidebar_collapsed(collapsed),
        UiCommand::OpenFind => open_find(shell),
        UiCommand::Quit => close_all_browsers(),
        UiCommand::ReopenClosedTab => reopen_closed_tab(shell),
        UiCommand::ZoomIn => change_zoom(shell, Some(true)),
        UiCommand::ZoomOut => change_zoom(shell, Some(false)),
        UiCommand::ZoomReset => change_zoom(shell, None),
        UiCommand::Print => print_page(shell),
        UiCommand::DevTools => show_dev_tools(shell),
        UiCommand::ToggleMuteTab { id } => toggle_mute(shell, id),
        UiCommand::SetExtensionPinned { id, pinned } => {
            extension_registry().set_pinned(&id, pinned);
            save_extension_registry();
            publish_extensions_everywhere();
        }
        UiCommand::SetExtensionEnabled { id, enabled } => {
            extension_registry().set_enabled(&id, enabled);
            save_extension_registry();
            publish_extensions_everywhere();
        }
        UiCommand::OpenExtensionPopup { id, left, top } => {
            open_extension_popup(shell, &id, left, top)
        }
        UiCommand::HideExtensionPopup => hide_extension_popup(shell),
        UiCommand::SelectTab { id } => select_tab(shell, id),
        UiCommand::CloseTab { id } => close_tab(shell, id),
        UiCommand::MoveTab { id, index } => {
            lock(shell).strip.move_to(id, index);
            publish(shell);
        }
        UiCommand::Navigate { input } => navigate(shell, &input),
        UiCommand::Back => go_in_history(shell, false),
        UiCommand::Forward => go_in_history(shell, true),
        UiCommand::Reload | UiCommand::Stop => {
            let Some(page) = active_browser(shell) else {
                return;
            };
            if command == UiCommand::Reload {
                page.reload();
            } else {
                page.stop_load();
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
    publish_bookmarks_bar();
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
            let items = with_storage(|storage| storage.bookmarks(&query, PAGE_LIST_LIMIT))
                .unwrap_or_default();
            let folders = with_storage(Storage::bookmark_folders).unwrap_or_default();
            reply(serde_json::json!({ "type": "bookmarks", "items": items, "folders": folders }));
        }
        PageCommand::UpdateBookmark {
            url,
            new_url,
            title,
            folder_id,
        } => {
            // The address came from a form, so it's checked like a typed homepage.
            let saved = normalize_homepage(&new_url).is_some_and(|new_url| {
                with_storage(|storage| {
                    storage.update_bookmark(&url, &new_url, title.trim(), folder_id)
                })
                .unwrap_or(false)
            });
            reply(serde_json::json!({ "type": "bookmarkSaved", "saved": saved }));
            refresh_bookmark_stars();
        }
        PageCommand::CreateFolder { name } => {
            if !name.trim().is_empty() {
                with_storage(|storage| storage.create_bookmark_folder(name.trim()));
                publish_bookmarks_bar();
            }
        }
        PageCommand::RenameFolder { id, name } => {
            if !name.trim().is_empty() {
                with_storage(|storage| storage.rename_bookmark_folder(id, name.trim()));
                publish_bookmarks_bar();
            }
        }
        PageCommand::DeleteFolder { id } => {
            with_storage(|storage| storage.delete_bookmark_folder(id));
            publish_bookmarks_bar();
        }
        PageCommand::ListDownloads => {
            let items: Vec<serde_json::Value> = with_storage(Storage::downloads)
                .unwrap_or_default()
                .iter()
                .map(saved_download_json)
                .collect();
            reply(serde_json::json!({ "type": "downloads", "items": items }));
        }
        // The page can only name files already in the saved list.
        PageCommand::OpenSavedDownload { path, confirmed } => {
            if is_saved_download(&path) {
                open_download_file(Path::new(&path), confirmed);
            }
        }
        PageCommand::ShowSavedDownload { path } => {
            if is_saved_download(&path) {
                show_in_folder(Path::new(&path));
            }
        }
        PageCommand::RemoveSavedDownload { path } => {
            with_storage(|storage| storage.remove_download(&path));
        }
        PageCommand::ClearSavedDownloads => {
            with_storage(Storage::clear_downloads);
        }
        PageCommand::SetSaveMemory { enabled } => {
            settings().save_memory = enabled;
            with_storage(|storage| {
                storage.set_setting(SETTING_SAVE_MEMORY, if enabled { "1" } else { "0" })
            });
        }
        PageCommand::SetVerticalTabs { enabled } => set_vertical_tabs(enabled),
        PageCommand::SetAskDownloadLocation { ask } => {
            settings().ask_download_location = ask;
            with_storage(|storage| {
                storage.set_setting(SETTING_ASK_DOWNLOAD_LOCATION, if ask { "1" } else { "0" })
            });
        }
        PageCommand::ClearData {
            history,
            cookies,
            cache,
            downloads,
        } => {
            clear_site_data(shell, cookies, cache);
            if history {
                with_storage(Storage::clear_history);
            }
            if downloads {
                with_storage(Storage::clear_downloads);
            }
            reply(serde_json::json!({ "type": "dataCleared" }));
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
        PageCommand::ListWorkspaces => reply(workspaces_event(None)),
        PageCommand::CreateWorkspace { name, color } => {
            let result = workspace_list().create(&name, &color);
            if result.is_ok() {
                save_workspace_list();
                publish_workspaces_everywhere();
            }
            reply(workspaces_event(result.err()));
        }
        PageCommand::RenameWorkspace { id, name } => {
            let result = workspace_list().rename(&id, &name);
            if result.is_ok() {
                save_workspace_list();
                publish_workspaces_everywhere();
            }
            reply(workspaces_event(result.err()));
        }
        PageCommand::SetWorkspaceColor { id, color } => {
            if workspace_list().set_color(&id, &color) {
                save_workspace_list();
                publish_workspaces_everywhere();
            }
            reply(workspaces_event(None));
        }
        PageCommand::DeleteWorkspace { id } => {
            let in_use = open_windows()
                .iter()
                .any(|window| lock(window).workspace == id);
            let error = if in_use {
                Some("Close that workspace's windows first.")
            } else if workspace_list().remove(&id) {
                save_workspace_list();
                let key = format!("{WORKSPACE_TABS_PREFIX}{id}");
                with_storage(|storage| storage.set_setting(&key, ""));
                saved_data()
                    .workspace_tabs
                    .lock()
                    .expect("Failed to lock workspace tabs")
                    .remove(&id);
                publish_workspaces_everywhere();
                None
            } else {
                Some("That workspace doesn't exist.")
            };
            reply(workspaces_event(error));
        }
        PageCommand::OpenWorkspace { id } => open_workspace(&id),
        PageCommand::ListExtensions => reply(extensions_event()),
        PageCommand::ChooseExtension { source } => choose_extension(frame, source),
        PageCommand::CancelInstallExtension { token } => {
            if let Some(prepared) = take_pending_extension(token) {
                let _ = std::fs::remove_dir_all(prepared.staging);
            }
        }
        PageCommand::ConfirmInstallExtension { token } => {
            if let (Some(prepared), Some(folder)) =
                (take_pending_extension(token), extensions_dir())
            {
                match extensions::install(prepared, &folder, now_ms(), &token.to_string()) {
                    Ok(entry) => {
                        extension_registry().add(entry);
                        save_extension_registry();
                        publish_extensions_everywhere();
                    }
                    Err(error) => reply(
                        serde_json::json!({ "type": "extensionError", "message": error.to_string() }),
                    ),
                }
            }
            reply(extensions_event());
        }
        PageCommand::SetExtensionEnabled { id, enabled } => {
            extension_registry().set_enabled(&id, enabled);
            save_extension_registry();
            publish_extensions_everywhere();
            reply(extensions_event());
        }
        PageCommand::SetExtensionPinned { id, pinned } => {
            extension_registry().set_pinned(&id, pinned);
            save_extension_registry();
            publish_extensions_everywhere();
            reply(extensions_event());
        }
        PageCommand::RemoveExtension { id } => {
            let removed = extension_registry().remove(&id);
            if let (Some(entry), Some(folder)) = (removed, extensions_dir()) {
                extensions::delete_files(&entry, &folder);
            }
            save_extension_registry();
            publish_extensions_everywhere();
            reply(extensions_event());
        }
        PageCommand::RestartBrowser => {
            crate::updates::restart_after_quit();
            close_all_browsers();
        }
        PageCommand::ListLogins => {
            reply(logins_event());
        }
        PageCommand::DeleteLogin { origin, username } => {
            passwords::delete(&origin, &username);
            with_storage(|storage| storage.delete_login(&origin, &username));
            refresh_login_keys();
            reply(logins_event());
        }
        PageCommand::ForgetNeverSaveLogin { origin } => {
            with_storage(|storage| storage.set_never_save_login(&origin, false));
            reply(logins_event());
        }
        PageCommand::ListSitePermissions => {
            let sites = site_permissions().sites().clone();
            reply(serde_json::json!({ "type": "sitePermissions", "sites": sites }));
        }
        PageCommand::ForgetSitePermissions { site } => {
            site_permissions().forget(&site);
            save_site_permissions();
            let sites = site_permissions().sites().clone();
            reply(serde_json::json!({ "type": "sitePermissions", "sites": sites }));
        }
        PageCommand::GetUpdateStatus => {
            reply(serde_json::json!({
                "type": "updateStatus",
                "version": crate::updates::VERSION,
                "status": crate::updates::status(),
            }));
        }
        PageCommand::CheckForUpdates => crate::updates::check_in_background(),
        PageCommand::RestartToUpdate => {
            if matches!(
                crate::updates::status(),
                crate::updates::Status::Ready { .. }
            ) {
                crate::updates::restart_after_quit();
                close_all_browsers();
            }
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
        Shortcut::CommandBar => open_command_bar(shell),
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
        Shortcut::Downloads => open_page(shell, InternalPage::Downloads),
        Shortcut::Bookmarks => open_page(shell, InternalPage::Bookmarks),
        Shortcut::Settings => open_page(shell, InternalPage::Settings),
        Shortcut::Quit => close_all_browsers(),
        Shortcut::NewWindow => open_window_in(
            None,
            &[BLANK_PAGE.to_owned()],
            is_private(shell),
            &workspace_of(shell),
        ),
        Shortcut::NewPrivateWindow => open_window(None, &[BLANK_PAGE.to_owned()], true),
        Shortcut::ToggleSidePanel => toggle_side_panel(shell),
        Shortcut::ClipPage => clip_page(shell),
        Shortcut::ReopenClosedTab => reopen_closed_tab(shell),
        Shortcut::ZoomIn => change_zoom(shell, Some(true)),
        Shortcut::ZoomOut => change_zoom(shell, Some(false)),
        Shortcut::ZoomReset => change_zoom(shell, None),
        Shortcut::Print => print_page(shell),
        Shortcut::DevTools => show_dev_tools(shell),
        Shortcut::CloseTab => {
            if let Some(id) = strip.active_id() {
                close_tab(shell, id);
            }
        }
        Shortcut::NextTab => select(strip.neighbor(1)),
        Shortcut::PreviousTab => select(strip.neighbor(-1)),
        Shortcut::TabAt(index) => select(strip.at(index)),
        Shortcut::LastTab => select(strip.last()),
        Shortcut::Minimize => window_action(shell, WindowAction::Minimize),
        Shortcut::ToggleFullscreen => window_action(shell, WindowAction::ToggleFullscreen),
        Shortcut::MoveTabLeft | Shortcut::MoveTabRight => {
            let offset = if shortcut == Shortcut::MoveTabLeft {
                -1
            } else {
                1
            };
            lock(shell).strip.move_active(offset);
            publish(shell);
        }
        // Escape stays with the page and address bar unless there's a load to stop.
        Shortcut::Stop if !strip.active().is_some_and(|tab| tab.page.is_loading) => return false,
        Shortcut::Back => go_in_history(shell, false),
        Shortcut::Forward => go_in_history(shell, true),
        Shortcut::Reload | Shortcut::HardReload | Shortcut::Stop => {
            let Some(page) = page else {
                return false;
            };
            match shortcut {
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
    if needs_close_confirmation(shell) {
        ask_to_confirm_close(shell);
        return false;
    }
    let (toolbar, pages) = {
        let mut shell = lock(shell);
        shell.window_closing = true;
        let pages: Vec<Browser> = shell
            .handles
            .values()
            .filter_map(|handles| handles.browser.clone())
            .collect();
        let mut others: Vec<Browser> = shell.popup.clone().into_iter().collect();
        others.extend(shell.ext_popup_browser.clone());
        others.extend(shell.toolbar.clone());
        (others, pages)
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
    shell.body = None;
    shell.sidebar_view = None;
    shell.side_panel_view = None;
    shell.layout = None;
    shell.popup_overlay = None;
    shell.popup_view = None;
    for handles in shell.handles.values_mut() {
        handles.browser = None;
    }
}

fn browser_closed(shell: &SharedShell, tab: Option<TabId>, closing: Option<&Browser>) {
    let window_closed = {
        let mut shell = lock(shell);
        if let Some(tab) = tab {
            // An unloaded tab may already have a new browser by the time the old one
            // finishes closing, so only the closing browser's handles go.
            let is_closing_browser = shell
                .handles
                .get(&tab)
                .and_then(|handles| handles.browser.as_ref())
                .zip(closing)
                .is_some_and(|(current, closing)| current.is_same(Some(&mut closing.clone())) != 0);
            if is_closing_browser {
                shell.handles.remove(&tab);
            }
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

        // The toolbar may open one window: the menu, with nothing loaded in it. It
        // starts hidden, so it can't take focus while someone types.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            target_url: Option<&CefString>,
            target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            let name = target_frame_name.map(CefString::to_string).unwrap_or_default();
            let is_menu = name == MENU_FRAME_NAME && (url.is_empty() || url == BLANK_PAGE);
            let has_menu = {
                let shell = lock(&self.shell);
                shell.popup.is_some() || shell.popup_view.is_some()
            };
            let Some(client) = client.filter(|_| is_menu && !has_menu) else {
                return 1;
            };
            *client = Some(PopupClient::new(self.shell.clone()));
            0
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed(&self.shell, None, None);
        }
    }
}

// The menu under the toolbar. It has no bridge of its own: it speaks through the
// toolbar that opened it, and it never navigates.
wrap_client! {
    pub struct PopupClient {
        shell: SharedShell,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(PopupLifeSpan::new(self.shell.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(Shortcuts::new(self.shell.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(MenuRequests::new())
        }

    }
}

wrap_life_span_handler! {
    struct PopupLifeSpan {
        shell: SharedShell,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let mut shell = lock(&self.shell);
            shell.popup = browser.cloned();
            shell.open_browsers += 1;
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            lock(&self.shell).popup = None;
            browser_closed(&self.shell, Some(TabId::MAX), None);
        }
    }
}

wrap_request_handler! {
    struct ToolbarRequests {}

    impl RequestHandler {
        // The toolbar only ever shows its own page, because the browser trusts every
        // message from it. Anything else, such as a dropped link, even to another
        // `data:` page, is refused.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let url = request.map(|request| CefString::from(&request.url()).to_string());
            (url.as_deref() != Some(toolbar_url())).into()
        }
    }
}

wrap_request_handler! {
    struct MenuRequests {}

    impl RequestHandler {
        // The toolbar writes the menu's page itself, so the menu never navigates.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            1
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
            Some(SitePermissionPrompts::new(self.shell.clone(), self.id))
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(PageDownloads::new(self.shell.clone(), self.id))
        }

        fn find_handler(&self) -> Option<FindHandler> {
            Some(PageFind::new(self.shell.clone(), self.id))
        }

        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            Some(PageDialogs::new(self.shell.clone(), self.id))
        }

        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(PageFocus::new(self.shell.clone(), self.id))
        }

        fn audio_handler(&self) -> Option<AudioHandler> {
            Some(PageAudio::new(self.shell.clone(), self.id))
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
            let name = CefString::from(&message.name()).to_string();
            let url = CefString::from(&frame.url()).to_string();
            if name == CLIP_RESULT_MESSAGE {
                if frame.is_main() != 0 {
                    let json = message
                        .argument_list()
                        .map(|list| CefString::from(&list.string(0)).to_string())
                        .unwrap_or_default();
                    finish_clip(&self.shell, self.id, &json);
                }
                return 1;
            }
            if name == LOGIN_SUBMITTED_MESSAGE {
                if frame.is_main() != 0 {
                    let json = message
                        .argument_list()
                        .map(|list| CefString::from(&list.string(0)).to_string())
                        .unwrap_or_default();
                    offer_to_save_login(&self.shell, self.id, &url, &json);
                }
                return 1;
            }
            if name != UI_COMMAND_MESSAGE {
                return 0;
            }
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

wrap_jsdialog_handler! {
    struct PageDialogs {
        shell: SharedShell,
        id: TabId,
    }

    impl JsdialogHandler {
        fn on_jsdialog(
            &self,
            _browser: Option<&mut Browser>,
            origin_url: Option<&CefString>,
            dialog_type: JsdialogType,
            message_text: Option<&CefString>,
            default_prompt_text: Option<&CefString>,
            callback: Option<&mut JsdialogCallback>,
            suppress_message: Option<&mut i32>,
        ) -> i32 {
            let kind = match sys::cef_jsdialog_type_t::from(dialog_type) {
                sys::cef_jsdialog_type_t::JSDIALOGTYPE_CONFIRM => "confirm",
                sys::cef_jsdialog_type_t::JSDIALOGTYPE_PROMPT => "prompt",
                _ => "alert",
            };
            let text = |value: Option<&CefString>| value.map(CefString::to_string).unwrap_or_default();
            let shown = callback.is_some_and(|callback| {
                show_dialog(
                    &self.shell,
                    self.id,
                    kind,
                    &text(origin_url),
                    &text(message_text),
                    &text(default_prompt_text),
                    PendingDialog::Page(callback.clone()),
                )
            });
            // A background tab can't interrupt, so its dialog is dropped.
            if !shown && let Some(suppress) = suppress_message {
                *suppress = 1;
            }
            shown.into()
        }

        fn on_before_unload_dialog(
            &self,
            _browser: Option<&mut Browser>,
            message_text: Option<&CefString>,
            _is_reload: i32,
            callback: Option<&mut JsdialogCallback>,
        ) -> i32 {
            let Some(callback) = callback else {
                return 0;
            };
            // A page that asks before leaving probably has unsaved work, so a tab
            // being unloaded to save memory stays loaded, without asking.
            if keep_discarding_tab_loaded(&self.shell, self.id) {
                callback.cont(0, None);
                return 1;
            }
            let message = message_text.map(CefString::to_string).unwrap_or_default();
            let pending = PendingDialog::Page(callback.clone());
            if !show_dialog(&self.shell, self.id, "leave", "", &message, "", pending) {
                // A background tab can't hold up closing.
                callback.cont(1, None);
            }
            1
        }

        fn on_reset_dialog_state(&self, _browser: Option<&mut Browser>) {
            let was_showing = {
                let mut shell = lock(&self.shell);
                // Only the page's own dialogs end when it navigates. A login form
                // usually navigates as it's sent, and its save prompt must stay.
                let is_ours = shell.dialog.as_ref().is_some_and(|(tab, pending)| {
                    *tab == self.id && matches!(pending, PendingDialog::Page(_))
                });
                if is_ours {
                    shell.dialog = None;
                }
                is_ours
            };
            if was_showing {
                close_dialog_bar(&self.shell);
            }
        }
    }
}

wrap_focus_handler! {
    struct PageFocus {
        shell: SharedShell,
        id: TabId,
    }

    impl FocusHandler {
        fn on_got_focus(&self, _browser: Option<&mut Browser>) {
            hide_extension_popup(&self.shell);
            // Clicking the other page of a split view makes it the one the toolbar shows.
            let switch = {
                let shell = lock(&self.shell);
                shell.split.is_some_and(|(left, right)| self.id == left || self.id == right)
                    && shell.strip.active_id() != Some(self.id)
            };
            if switch {
                select_tab(&self.shell, self.id);
            }
            let toolbar = lock(&self.shell).toolbar.clone();
            send_to_toolbar(toolbar.as_ref(), &UiEvent::PageFocused);
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
            let is_retry = existing.is_some();
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
                        is_risky: is_risky_file(&path.to_string_lossy()),
                    });
                    path
                }
            };
            // Asking only applies to a new download, not to Chromium retrying one.
            let ask = !is_retry && settings().ask_download_location;
            callback.cont(Some(&CefString::from(path.to_string_lossy().as_ref())), ask.into());
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
                let actual_path = CefString::from(&item.full_path()).to_string();
                if !actual_path.is_empty() {
                    shell.downloads.set_path(id, PathBuf::from(actual_path));
                }
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
            if matches!(state, DownloadState::Complete | DownloadState::Failed) {
                let record = lock(&self.shell).downloads.get(item.id()).map(|item| DownloadRecord {
                    path: item.path.to_string_lossy().into_owned(),
                    file_name: item.file_name.clone(),
                    state: state_name(item.state).to_owned(),
                    received_bytes: item.received_bytes,
                    total_bytes: item.total_bytes,
                    finished_ms: now_ms(),
                });
                if let Some(record) = record.filter(|_| !is_private(&self.shell)) {
                    with_storage(|storage| storage.save_download(&record));
                }
            }
            if state != DownloadState::InProgress {
                close_hidden_tab(&self.shell, self.id);
            }
        }
    }
}

// There's no permission prompt UI yet. Without an answer, pages wait forever.
wrap_permission_handler! {
    struct SitePermissionPrompts {
        shell: SharedShell,
        id: TabId,
    }

    impl PermissionHandler {
        // The camera and microphone. Sharing the screen isn't supported, because it
        // needs a picker for what to share.
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> i32 {
            let Some(callback) = callback else {
                return 0;
            };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let mut permissions = Vec::new();
            if requested_permissions & MEDIA_VIDEO != 0 {
                permissions.push(Permission::Camera);
            }
            if requested_permissions & MEDIA_AUDIO != 0 {
                permissions.push(Permission::Microphone);
            }
            let only_devices = requested_permissions & !(MEDIA_AUDIO | MEDIA_VIDEO) == 0;
            let answer = if only_devices {
                let pending_callback = callback.clone();
                ask_permission(&self.shell, self.id, &origin, permissions, |site, permissions| {
                    PendingDialog::Media {
                        callback: pending_callback,
                        requested: requested_permissions,
                        site,
                        permissions,
                    }
                })
            } else {
                Some(false)
            };
            match answer {
                Some(true) => callback.cont(requested_permissions),
                Some(false) => callback.cancel(),
                None => {}
            }
            1
        }

        // Location, and the camera and microphone when a page asks ahead of using them.
        // Anything else, including notifications, which this browser can't show, is
        // refused.
        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> i32 {
            let Some(callback) = callback else {
                return 0;
            };
            let origin = requesting_origin.map(CefString::to_string).unwrap_or_default();
            let known = [
                (PROMPT_CAMERA, Permission::Camera),
                (PROMPT_MICROPHONE, Permission::Microphone),
                (PROMPT_LOCATION, Permission::Location),
            ];
            let permissions: Vec<Permission> = known
                .iter()
                .filter(|(bit, _)| requested_permissions & bit != 0)
                .map(|(_, permission)| *permission)
                .collect();
            let all_known = known
                .iter()
                .fold(requested_permissions, |rest, (bit, _)| rest & !bit)
                == 0;
            let answer = if all_known {
                let pending_callback = callback.clone();
                ask_permission(&self.shell, self.id, &origin, permissions, |site, permissions| {
                    PendingDialog::Prompt {
                        callback: pending_callback,
                        id: prompt_id,
                        site,
                        permissions,
                    }
                })
            } else {
                Some(false)
            };
            match answer {
                Some(true) => callback.cont(PermissionRequestResult::ACCEPT),
                Some(false) => callback.cont(PermissionRequestResult::DENY),
                None => {}
            }
            1
        }

        // The page went away or stopped asking, so its question goes too.
        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            _result: PermissionRequestResult,
        ) {
            let is_this_prompt = lock(&self.shell).dialog.as_ref().is_some_and(|(tab, pending)| {
                *tab == self.id && matches!(pending, PendingDialog::Prompt { id, .. } if *id == prompt_id)
            });
            if is_this_prompt {
                lock(&self.shell).dialog = None;
                close_dialog_bar(&self.shell);
            }
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
            let closes_find = {
                let shell = lock(&self.shell);
                let previous = shell.handles.get(&self.id).map(|handles| handles.committed_url.as_str());
                // Matches from the old page mean nothing on a new one. A change of
                // only the part after # stays on the same page.
                let page_of = |url: &str| url.split('#').next().unwrap_or_default().to_owned();
                shell.find_bar_shown
                    && shell.strip.active_id() == Some(self.id)
                    && previous.is_some_and(|previous| !previous.is_empty() && page_of(previous) != page_of(&url))
            };
            if closes_find {
                close_find(&self.shell, false);
            }
            let is_bookmarked = {
                let mut shell = lock(&self.shell);
                if let Some(handles) = shell.handles.get_mut(&self.id) {
                    handles.committed_url = url.clone();
                    handles.showing_error = false;
                }
                // Page loads are recorded when they finish, in case they fail. This is
                // for sites that change their address without loading a new page.
                let is_loading = shell.strip.get(self.id).is_some_and(|tab| tab.page.is_loading);
                if !is_loading && is_web_url(&url) && !shell.private {
                    shell.with_storage(|storage| storage.record_visit(&url, "", now_ms()));
                }
                shell
                    .with_storage(|storage| storage.is_bookmarked(&url))
                    .unwrap_or(false)
            };
            let own_list = {
                let mut shell = lock(&self.shell);
                shell.handles.get_mut(&self.id).and_then(|handles| {
                    let history = handles.history.as_mut()?;
                    if url != BLANK_PAGE {
                        history.arrived(&url);
                    }
                    Some(history.clone())
                })
            };
            let has_logins = has_saved_logins(&workspace_of(&self.shell), &url);
            update_tab(&self.shell, self.id, |tab| {
                tab.page.url = url;
                tab.page.is_bookmarked = is_bookmarked;
                tab.page.has_logins = has_logins;
                if let Some(history) = own_list {
                    tab.page.can_go_back = history.can_go_back();
                    tab.page.can_go_forward = history.can_go_forward();
                    tab.history_entries = history.entries;
                    tab.history_index = history.index;
                }
            });
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let urls: Vec<String> = icon_urls
                .map(std::mem::take)
                .map(|list| list.into_iter().collect())
                .unwrap_or_default();
            // Only web addresses of a sensible length: a long `data:` address would be
            // sent with every update.
            let Some(url) = urls
                .into_iter()
                .find(|url| is_web_url(url) && url.len() <= 1024)
            else {
                return;
            };
            update_tab(&self.shell, self.id, |tab| tab.favicon = Some(url.clone()));
            if let Some(host) = browser.and_then(|browser| browser.host()) {
                request_favicon(&self.shell, &host, url);
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            let (window, is_active) = {
                let shell = lock(&self.shell);
                (shell.window.clone(), shell.strip.active_id() == Some(self.id))
            };
            let title = title.map(CefString::to_string).unwrap_or_default();
            if is_active && let Some(window) = window {
                let shown = if title == BLANK_PAGE { "New tab" } else { &title };
                let shown = window_title(&self.shell, shown);
                window.set_title(Some(&CefString::from(shown.as_str())));
            }
            {
                let shell = lock(&self.shell);
                let showing_error = shell.handles.get(&self.id).is_some_and(|handles| handles.showing_error);
                if let Some(tab) = shell
                    .strip
                    .get(self.id)
                    .filter(|tab| !showing_error && !shell.private && is_web_url(&tab.page.url))
                {
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
            shell.strip.set_opener(id, self.id);
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
            if lock(&self.shell).discarding.remove(&self.id) {
                finish_discard(&self.shell, self.id);
                return 1;
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

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            browser_closed(&self.shell, Some(self.id), browser.map(|browser| &*browser));
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
            if is_loading == 0
                && let Some(browser) = _browser
            {
                capture_history(&self.shell, self.id, &browser.clone());
            }
            let committed_url = lock(&self.shell)
                .handles
                .get(&self.id)
                .map(|handles| handles.committed_url.clone())
                .unwrap_or_default();
            let own_list = lock(&self.shell)
                .handles
                .get(&self.id)
                .and_then(|handles| handles.history.clone());
            update_tab(&self.shell, self.id, |tab| {
                tab.page.is_loading = is_loading != 0;
                tab.page.can_go_back = own_list.as_ref().map_or(can_go_back != 0, TabHistory::can_go_back);
                tab.page.can_go_forward = own_list.as_ref().map_or(can_go_forward != 0, TabHistory::can_go_forward);
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
            refresh_zoom(&self.shell, self.id);
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
                    .filter(|tab| http_status_code > 0 && is_web_url(&tab.page.url) && !shell.private);
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

wrap_audio_handler! {
    struct PageAudio {
        shell: SharedShell,
        id: TabId,
    }

    impl AudioHandler {
        fn on_audio_stream_started(
            &self,
            _browser: Option<&mut Browser>,
            _params: Option<&AudioParameters>,
            _channels: i32,
        ) {
            lock(&self.shell).audible.insert(self.id);
            update_tab(&self.shell, self.id, |tab| tab.audible = true);
        }

        fn on_audio_stream_stopped(&self, _browser: Option<&mut Browser>) {
            lock(&self.shell).audible.remove(&self.id);
            update_tab(&self.shell, self.id, |tab| tab.audible = false);
        }

        fn on_audio_stream_error(&self, _browser: Option<&mut Browser>, _message: Option<&CefString>) {
            lock(&self.shell).audible.remove(&self.id);
            update_tab(&self.shell, self.id, |tab| tab.audible = false);
        }
    }
}

wrap_task! {
    struct DiscardCheck;

    impl Task {
        fn execute(&self) {
            discard_inactive_tabs();
            schedule_discard_check();
        }
    }
}

/// Starts checking for tabs to unload. Call once, on the UI thread.
pub fn schedule_discard_check() {
    post_delayed_task(
        ThreadId::UI,
        Some(&mut DiscardCheck::new()),
        DISCARD_CHECK_MS,
    );
}

/// Unloads background tabs across every window that haven't been used for a while,
/// or the least recently used ones when too many are loaded.
fn discard_inactive_tabs() {
    if !settings().save_memory || QUITTING.load(Ordering::Relaxed) {
        return;
    }
    let windows = open_windows();
    let mut loaded = Vec::new();
    for (window, shell) in windows.iter().enumerate() {
        let shell = lock(shell);
        if shell.window_closing {
            continue;
        }
        let dialog_tab = shell.dialog.as_ref().map(|(tab, _)| *tab);
        // A download belongs to the tab that started it and would stop with it, and
        // which tab that was isn't tracked, so a window with downloads keeps its tabs.
        let downloading = shell.downloads.active_count() > 0;
        for tab in shell.strip.tabs() {
            let has_browser = shell
                .handles
                .get(&tab.id)
                .is_some_and(|handles| handles.browser.is_some());
            if !has_browser
                || shell.unloaded.contains_key(&tab.id)
                || shell.discarding.contains(&tab.id)
            {
                continue;
            }
            let is_blank = tab.page.url.is_empty() || tab.page.url == BLANK_PAGE;
            let protected = Some(tab.id) == shell.strip.active_id()
                || shell.audible.contains(&tab.id)
                || tab.page.is_loading
                || Some(tab.id) == dialog_tab
                || shell.keep_loaded.contains(&tab.id)
                || shell
                    .split
                    .is_some_and(|(left, right)| tab.id == left || tab.id == right)
                || downloading
                || is_blank;
            loaded.push(LoadedTab {
                key: (window, tab.id),
                last_shown_ms: shell
                    .last_shown
                    .get(&tab.id)
                    .copied()
                    .unwrap_or_else(now_ms),
                protected,
            });
        }
    }
    for (window, id) in tabs_to_discard(&loaded, now_ms(), DISCARD_IDLE_MS, DISCARD_LOADED_LIMIT) {
        discard_tab(&windows[window], id);
    }
}

/// Closes a background tab's browser and keeps the tab, ready to load again when
/// it's selected, like a tab restored at startup.
fn discard_tab(shell: &SharedShell, id: TabId) {
    let browser = {
        let mut guard = lock(shell);
        let Some(tab) = guard.strip.get(id).cloned() else {
            return;
        };
        let Some(browser) = guard
            .handles
            .get(&id)
            .and_then(|handles| handles.browser.clone())
        else {
            return;
        };
        if tab.history_entries.len() > 1 {
            let history = TabHistory::new(tab.history_entries.clone(), tab.history_index);
            guard.restored_history.insert(id, history);
        }
        guard.unloaded.insert(id, tab.page.url.clone());
        guard.discarding.insert(id);
        guard.audible.remove(&id);
        browser
    };
    if let Some(host) = browser.host() {
        // Not forced: a forced close never finishes for a browser in a window's view.
        // If the page asks before leaving, the tab stays loaded instead.
        host.close_browser(0);
    }
}

/// Called when a page asks before leaving. Returns whether the tab was being unloaded,
/// in which case it now stays loaded.
fn keep_discarding_tab_loaded(shell: &SharedShell, id: TabId) -> bool {
    let mut guard = lock(shell);
    if !guard.discarding.remove(&id) {
        return false;
    }
    guard.unloaded.remove(&id);
    guard.restored_history.remove(&id);
    guard.keep_loaded.insert(id);
    true
}

/// Takes an unloaded tab's view out of the window. Its place in the strip stays.
fn finish_discard(shell: &SharedShell, id: TabId) {
    let (window, view) = {
        let mut guard = lock(shell);
        // As when closing a tab, dropping these references is what lets the browser
        // finish closing and its renderer process exit.
        let view = guard.handles.remove(&id).map(|handles| handles.view);
        (guard.body.clone(), view)
    };
    if let (Some(body), Some(view)) = (window, view) {
        body.remove_child_view(Some(&mut View::from(&view)));
    }
    update_tab(shell, id, |tab| {
        tab.page.is_loading = false;
        tab.page.progress = 0.0;
    });
}

/// Loads the window's other requested tabs without showing them.
pub fn load_background_tabs(shell: &SharedShell) {
    let ids = std::mem::take(&mut lock(shell).background_loads);
    for id in ids {
        let Some(url) = lock(shell).unloaded.remove(&id) else {
            continue;
        };
        create_view(shell, id, &url);
        let (body, layout, view) = {
            let guard = lock(shell);
            (
                guard.body.clone(),
                guard.layout.clone(),
                guard.handles.get(&id).map(|handles| handles.view.clone()),
            )
        };
        if let (Some(body), Some(view)) = (body, view) {
            let mut view = View::from(&view);
            view.set_visible(0);
            body.add_child_view(Some(&mut view));
            if let Some(layout) = layout {
                layout.set_flex_for_view(Some(&mut view), 1);
            }
        }
    }
    publish(shell);
}

// -- Split view ------------------------------------------------------------------------

/// Puts the views in the pages box in order: the two panes of a split view, left before
/// right, and the side panel last, whatever order they were added in.
fn order_split_panes(shell: &SharedShell) {
    let (body, left, right, panel) = {
        let guard = lock(shell);
        let pair = guard.split;
        let view_of = |id: TabId| guard.handles.get(&id).map(|handles| handles.view.clone());
        (
            guard.body.clone(),
            pair.and_then(|(left, _)| view_of(left)),
            pair.and_then(|(_, right)| view_of(right)),
            guard.side_panel_view.clone(),
        )
    };
    let Some(body) = body else {
        return;
    };
    let last = body.child_view_count() as i32 - 1;
    if let (Some(left), Some(right)) = (left, right) {
        body.reorder_child_view(Some(&mut View::from(&left)), last);
        body.reorder_child_view(Some(&mut View::from(&right)), last);
    }
    if let Some(panel) = panel {
        body.reorder_child_view(Some(&mut View::from(&panel)), last);
    }
}

/// Shows a new tab beside the current one, or, when two are shown, goes back to one.
fn toggle_split_view(shell: &SharedShell) {
    let (is_split, active) = {
        let guard = lock(shell);
        (guard.split.is_some(), guard.strip.active_id())
    };
    let Some(active) = active else {
        return;
    };
    if is_split {
        lock(shell).split = None;
        // Shows the active tab alone again, and hides the other.
        select_tab(shell, active);
        return;
    }
    let id = create_tab(shell, BLANK_PAGE);
    lock(shell).split = Some((active, id));
    attach_tab(shell, id);
}

// -- The side panel -------------------------------------------------------------------

fn panel_data() -> MutexGuard<'static, SidePanelData> {
    saved_data()
        .panel
        .lock()
        .expect("Failed to lock the side panel's data")
}

fn save_panel_data() {
    let json = panel_data().to_json();
    with_storage(|storage| storage.set_setting(SETTING_SIDE_PANEL, &json));
}

fn side_panel_of(shell: &SharedShell) -> Option<Browser> {
    let guard = lock(shell);
    guard.side_panel.clone().filter(|_| guard.side_panel_ready)
}

/// Sends the notes, to-dos and snippets to this window's side panel, if it's open.
fn publish_panel(shell: &SharedShell) {
    let Some(panel) = side_panel_of(shell) else {
        return;
    };
    let data = panel_data().clone();
    send_to_toolbar(Some(&panel), &UiEvent::PanelData { data: &data });
}

/// Changes are shown in every window's panel, since they all share the same notes.
fn publish_panel_everywhere() {
    for shell in open_windows() {
        publish_panel(&shell);
    }
}

/// Tells the toolbar whether its window's panel is open, for the button's state.
fn publish_side_panel_state(shell: &SharedShell) {
    let (toolbar, open) = {
        let guard = lock(shell);
        (
            guard.toolbar.clone().filter(|_| guard.toolbar_ready),
            guard.side_panel_view.is_some(),
        )
    };
    send_to_toolbar(toolbar.as_ref(), &UiEvent::SidePanel { open });
}

/// Opens the side panel at the right of the pages, or closes it.
fn toggle_side_panel(shell: &SharedShell) {
    let (body, view) = {
        let guard = lock(shell);
        (guard.body.clone(), guard.side_panel_view.clone())
    };
    let Some(body) = body else {
        return;
    };
    hide_extension_popup(shell);
    if view.is_none() {
        let mut client = SidebarClient::new(shell.clone(), Dock::Panel);
        let created = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(side_panel_url())),
            Some(&BrowserSettings::default()),
            None,
            None,
            Some(&mut SidebarViewHost::new(Dock::Panel)),
        );
        if let Some(created) = created {
            body.add_child_view(Some(&mut View::from(&created)));
            lock(shell).side_panel_view = Some(created);
        }
    } else {
        let (view, browser) = {
            let mut guard = lock(shell);
            guard.side_panel_ready = false;
            (guard.side_panel_view.take(), guard.side_panel.clone())
        };
        if let Some(view) = &view {
            body.remove_child_view(Some(&mut View::from(view)));
        }
        // The browser's closing calls back into code that locks, so nothing's held here.
        if let Some(host) = browser.and_then(|browser| browser.host()) {
            host.close_browser(1);
        }
    }
    order_split_panes(shell);
    let window = lock(shell).window.clone();
    if let Some(window) = window {
        window.layout();
    }
    publish_side_panel_state(shell);
}

/// Saves the active page's title and address as a snippet.
fn add_current_page_snippet(shell: &SharedShell) {
    let page = {
        let guard = lock(shell);
        guard
            .strip
            .active_id()
            .and_then(|id| guard.strip.get(id))
            .filter(|tab| is_web_url(&tab.page.url))
            .map(|tab| (tab.title.clone(), tab.page.url.clone()))
    };
    if let Some((title, url)) = page
        && panel_data().add_snippet(&title, &url)
    {
        save_panel_data();
        publish_panel_everywhere();
    }
}

/// Shows a short message in the toolbar.
fn toast(shell: &SharedShell, text: &str) {
    let toolbar = {
        let guard = lock(shell);
        guard.toolbar.clone().filter(|_| guard.toolbar_ready)
    };
    send_to_toolbar(toolbar.as_ref(), &UiEvent::Toast { text });
}

/// Saves the active page as a clip, with the text selected on it, or else its description.
fn clip_page(shell: &SharedShell) {
    let (private, page) = {
        let guard = lock(shell);
        let page = guard
            .strip
            .active_id()
            .and_then(|id| guard.strip.get(id))
            .filter(|tab| is_web_url(&tab.page.url))
            .map(|tab| tab.id);
        (guard.private, page)
    };
    // What a private window does isn't kept, and clips are kept.
    if private {
        toast(shell, "Clips aren't kept in private windows");
        return;
    }
    let Some(id) = page else {
        toast(shell, "Only web pages can be clipped");
        return;
    };
    let frame = active_browser(shell).and_then(|browser| browser.main_frame());
    let message = process_message_create(Some(&CefString::from(CLIP_REQUEST_MESSAGE)));
    let (Some(frame), Some(mut message)) = (frame, message) else {
        lock(shell).pending_clip = Some(id);
        finish_clip(shell, id, "");
        return;
    };
    lock(shell).pending_clip = Some(id);
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
    // A page that never answers still gets saved, just without text.
    post_delayed_task(
        ThreadId::UI,
        Some(&mut ClipTimeout::new(shell.clone(), id)),
        1500,
    );
}

wrap_task! {
    struct ClipTimeout {
        shell: SharedShell,
        id: TabId,
    }

    impl Task {
        fn execute(&self) {
            finish_clip(&self.shell, self.id, "");
        }
    }
}

/// Saves the clip once the page has answered, or the wait is over. Only a clip that was
/// asked for is accepted, and only once, so a page can't add clips on its own.
fn finish_clip(shell: &SharedShell, id: TabId, json: &str) {
    let page = {
        let mut guard = lock(shell);
        if guard.pending_clip != Some(id) {
            return;
        }
        guard.pending_clip = None;
        guard
            .strip
            .get(id)
            .filter(|tab| is_web_url(&tab.page.url))
            .map(|tab| (tab.title.clone(), tab.page.url.clone()))
    };
    let Some((title, url)) = page else {
        return;
    };
    let read: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let text = |key: &str| read.get(key).and_then(|value| value.as_str()).unwrap_or("");
    let selection = text("selection").trim();
    let note = if selection.is_empty() {
        text("description")
    } else {
        selection
    };
    if panel_data().add_clip(&title, &url, note, now_ms()) {
        save_panel_data();
        publish_panel_everywhere();
        toast(shell, "Clipped");
    }
}

/// Runs a change to the panel's data, then saves and shows it everywhere if it changed.
fn change_panel_data(change: impl FnOnce(&mut SidePanelData) -> bool) {
    if change(&mut panel_data()) {
        save_panel_data();
        publish_panel_everywhere();
    }
}

// -- The vertical tab list -------------------------------------------------------------

/// Which docked page a client belongs to. Both are small pages of the browser's own that
/// sit beside the pages, and each is held to the commands it needs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Dock {
    Tabs,
    Panel,
}

/// The vertical tab list's browser, once its page has loaded and can take events.
fn sidebar_of(shell: &SharedShell) -> Option<Browser> {
    let guard = lock(shell);
    guard.sidebar.clone().filter(|_| guard.sidebar_ready)
}

fn sidebar_width() -> i32 {
    if settings().sidebar_collapsed {
        SIDEBAR_COLLAPSED_WIDTH
    } else {
        SIDEBAR_WIDTH
    }
}

/// Tells the toolbar and the tab list where the tabs are shown.
fn publish_layout(shell: &SharedShell) {
    let (vertical, collapsed) = {
        let settings = settings();
        (settings.vertical_tabs, settings.sidebar_collapsed)
    };
    let toolbar = {
        let guard = lock(shell);
        guard.toolbar.clone().filter(|_| guard.toolbar_ready)
    };
    let event = UiEvent::Layout {
        vertical,
        collapsed,
    };
    send_to_toolbar(toolbar.as_ref(), &event);
    send_to_toolbar(sidebar_of(shell).as_ref(), &event);
}

/// Makes the window match the setting: adds the tab list beside the pages when tabs are
/// vertical, or takes it away when they aren't, and resizes the toolbar to suit.
pub fn sync_sidebar(shell: &SharedShell) {
    let vertical = settings().vertical_tabs;
    let (body, has_view) = {
        let guard = lock(shell);
        (guard.body.clone(), guard.sidebar_view.is_some())
    };
    let Some(body) = body else {
        return;
    };
    if vertical && !has_view {
        let mut client = SidebarClient::new(shell.clone(), Dock::Tabs);
        let view = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(sidebar_url())),
            Some(&BrowserSettings::default()),
            None,
            None,
            Some(&mut SidebarViewHost::new(Dock::Tabs)),
        );
        if let Some(view) = view {
            // First in the box, so the pages take the rest of the width.
            body.add_child_view_at(Some(&mut View::from(&view)), 0);
            lock(shell).sidebar_view = Some(view);
        }
    } else if !vertical && has_view {
        let (view, browser) = {
            let mut guard = lock(shell);
            guard.sidebar_ready = false;
            (guard.sidebar_view.take(), guard.sidebar.clone())
        };
        if let Some(view) = &view {
            body.remove_child_view(Some(&mut View::from(view)));
        }
        // The browser's closing calls back into code that locks, so nothing's held here.
        if let Some(host) = browser.and_then(|browser| browser.host()) {
            host.close_browser(1);
        }
    }
    resize_toolbar(shell);
    publish_layout(shell);
}

/// Applies a change of the setting to every window.
fn sync_sidebars_everywhere() {
    for shell in open_windows() {
        sync_sidebar(&shell);
    }
}

/// Resizes the tab list in every window, after it was collapsed or widened.
fn resize_sidebars_everywhere() {
    for shell in open_windows() {
        let (view, window) = {
            let guard = lock(&shell);
            (guard.sidebar_view.clone(), guard.window.clone())
        };
        if let Some(view) = view {
            View::from(&view).invalidate_layout();
        }
        if let Some(window) = window {
            window.layout();
        }
        publish_layout(&shell);
    }
}

fn set_vertical_tabs(enabled: bool) {
    {
        let mut settings = settings();
        if settings.vertical_tabs == enabled {
            return;
        }
        settings.vertical_tabs = enabled;
    }
    with_storage(|storage| {
        storage.set_setting(SETTING_VERTICAL_TABS, if enabled { "1" } else { "0" })
    });
    // Not now: this runs inside a message from a toolbar, and that window's toolbar
    // wouldn't take its new height until the message was done.
    post_task(ThreadId::UI, Some(&mut SyncSidebars::new()));
}

wrap_task! {
    struct SyncSidebars;

    impl Task {
        fn execute(&self) {
            sync_sidebars_everywhere();
        }
    }
}

fn set_sidebar_collapsed(collapsed: bool) {
    {
        let mut settings = settings();
        if settings.sidebar_collapsed == collapsed {
            return;
        }
        settings.sidebar_collapsed = collapsed;
    }
    with_storage(|storage| {
        storage.set_setting(SETTING_SIDEBAR_COLLAPSED, if collapsed { "1" } else { "0" })
    });
    resize_sidebars_everywhere();
}

wrap_browser_view_delegate! {
    struct SidebarViewHost {
        dock: Dock,
    }

    impl ViewDelegate {
        // Only the width matters: the box stretches the view to the window's height.
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            let width = match self.dock {
                Dock::Tabs => sidebar_width(),
                Dock::Panel => SIDE_PANEL_WIDTH,
            };
            Size { width, height: 100 }
        }
    }

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

// The vertical tab list's client. The browser trusts what it sends, so it's held to
// the commands a tab list needs.
wrap_client! {
    pub struct SidebarClient {
        shell: SharedShell,
        dock: Dock,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(SidebarLifeSpan::new(self.shell.clone(), self.dock))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(Shortcuts::new(self.shell.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(SidebarRequests::new(self.dock))
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
            match (self.dock, serde_json::from_str::<UiCommand>(&json)) {
                (Dock::Tabs, Ok(UiCommand::SidebarReady)) => {
                    lock(&self.shell).sidebar_ready = true;
                    publish(&self.shell);
                    publish_layout(&self.shell);
                    publish_workspaces(&self.shell);
                    send_known_favicons(&self.shell);
                }
                (Dock::Panel, Ok(UiCommand::SidePanelReady)) => {
                    lock(&self.shell).side_panel_ready = true;
                    publish_panel(&self.shell);
                }
                (Dock::Tabs, Ok(command)) if command.allowed_from_sidebar() => {
                    run_command(&self.shell, command)
                }
                (Dock::Panel, Ok(command)) if command.allowed_from_side_panel() => {
                    run_command(&self.shell, command)
                }
                _ => {}
            }
            1
        }
    }
}

wrap_life_span_handler! {
    struct SidebarLifeSpan {
        shell: SharedShell,
        dock: Dock,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let mut shell = lock(&self.shell);
            match self.dock {
                Dock::Tabs => shell.sidebar = browser.cloned(),
                Dock::Panel => shell.side_panel = browser.cloned(),
            }
            shell.open_browsers += 1;
        }

        // The tab list opens no windows.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            1
        }

        // Like a tab, the list's view lives in the window, which must stay open when it
        // closes. Letting go of the view is what lets the browser finish.
        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            let view = {
                let mut shell = lock(&self.shell);
                if shell.window_closing {
                    return 0;
                }
                match self.dock {
                    Dock::Tabs => {
                        shell.sidebar_ready = false;
                        shell.sidebar_view.take()
                    }
                    Dock::Panel => {
                        shell.side_panel_ready = false;
                        shell.side_panel_view.take()
                    }
                }
            };
            drop(view);
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            {
                let mut shell = lock(&self.shell);
                match self.dock {
                    Dock::Tabs => {
                        shell.sidebar = None;
                        shell.sidebar_ready = false;
                    }
                    Dock::Panel => {
                        shell.side_panel = None;
                        shell.side_panel_ready = false;
                    }
                }
            }
            browser_closed(&self.shell, Some(TabId::MAX), None);
        }
    }
}

wrap_request_handler! {
    struct SidebarRequests {
        dock: Dock,
    }

    impl RequestHandler {
        // The tab list only ever shows its own page, because the browser acts on what
        // it sends. A dropped link, even to another `data:` page, is refused.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let url = request.map(|request| CefString::from(&request.url()).to_string());
            let own = match self.dock {
                Dock::Tabs => sidebar_url(),
                Dock::Panel => side_panel_url(),
            };
            (url.as_deref() != Some(own)).into()
        }
    }
}

// -- Things the toolbar and its menu ask for ---------------------------------------------

/// Brings back the tab closed most recently in this window.
fn reopen_closed_tab(shell: &SharedShell) {
    let closed = lock(shell).closed_tabs.pop();
    if let Some((url, _)) = closed {
        let id = create_tab(shell, &url);
        attach_tab(shell, id);
    }
}

/// Zooms the page in or out one step, or back to 100% with `None`.
fn change_zoom(shell: &SharedShell, zoom_in: Option<bool>) {
    let Some(host) = active_browser(shell).and_then(|browser| browser.host()) else {
        return;
    };
    let current = percent_for_level(host.zoom_level());
    let target = zoom_in.map_or(100, |up| next_zoom_percent(current, up));
    host.set_zoom_level(level_for_percent(target));
    let id = lock(shell).strip.active_id();
    if let Some(id) = id {
        update_tab(shell, id, |tab| tab.page.zoom_percent = target);
    }
}

/// Reads a tab's zoom, which CEF keeps for each site, into what the toolbar shows.
fn refresh_zoom(shell: &SharedShell, id: TabId) {
    let host = lock(shell)
        .handles
        .get(&id)
        .and_then(|handles| handles.browser.clone())
        .and_then(|browser| browser.host());
    let Some(host) = host else {
        return;
    };
    let percent = percent_for_level(host.zoom_level());
    let changed = lock(shell)
        .strip
        .get(id)
        .is_some_and(|tab| tab.page.zoom_percent != percent);
    if changed {
        update_tab(shell, id, |tab| tab.page.zoom_percent = percent);
    }
}

fn print_page(shell: &SharedShell) {
    if let Some(host) = active_browser(shell).and_then(|browser| browser.host()) {
        host.print();
    }
}

fn show_dev_tools(shell: &SharedShell) {
    if let Some(host) = active_browser(shell).and_then(|browser| browser.host()) {
        host.show_dev_tools(Some(&WindowInfo::default()), None, None, None);
    }
}

fn toggle_mute(shell: &SharedShell, id: TabId) {
    let host = lock(shell)
        .handles
        .get(&id)
        .and_then(|handles| handles.browser.clone())
        .and_then(|browser| browser.host());
    let Some(host) = host else {
        return;
    };
    let muted = host.is_audio_muted() == 0;
    host.set_audio_muted(muted.into());
    update_tab(shell, id, |tab| tab.muted = muted);
}

// -- Extension buttons -------------------------------------------------------------------

/// The extensions that are on, for the toolbar's extensions menu and pinned buttons.
fn extension_items() -> Vec<serde_json::Value> {
    let Some(folder) = extensions_dir() else {
        return Vec::new();
    };
    extension_registry()
        .describe(&folder)
        .into_iter()
        .filter(|info| info.enabled)
        .map(|info| {
            serde_json::json!({
                "id": info.id,
                "name": info.summary.name,
                "title": info.summary.action_title,
                "iconData": info.icon_data,
                "popupUrl": info.popup_url,
                "pinned": info.pinned,
            })
        })
        .collect()
}

fn publish_extensions(shell: &SharedShell) {
    let (toolbar, available) = {
        let shell = lock(shell);
        (
            shell.toolbar.clone().filter(|_| shell.toolbar_ready),
            !shell.private && shell.workspace.is_empty(),
        )
    };
    // Private windows and other workspaces don't run extensions, so they get none.
    let items = if available {
        extension_items()
    } else {
        Vec::new()
    };
    send_to_toolbar(
        toolbar.as_ref(),
        &UiEvent::Extensions {
            items: &items,
            available,
        },
    );
}

fn publish_extensions_everywhere() {
    for shell in open_windows() {
        publish_extensions(&shell);
    }
}

/// The address of an extension's popup, if that extension is on and has one. It's built
/// from what the browser itself saved, never from what a page or the toolbar sent.
fn extension_popup_url(id: &str) -> Option<String> {
    let folder = extensions_dir()?;
    extension_registry()
        .describe(&folder)
        .into_iter()
        .find(|info| info.enabled && info.id == id)
        .and_then(|info| info.popup_url)
}

/// Shows an extension's popup page under the toolbar, in a browser of its own. A second
/// press on the same extension puts it away.
fn open_extension_popup(shell: &SharedShell, id: &str, left: i32, top: i32) {
    let (window, same, private) = {
        let guard = lock(shell);
        (
            guard.window.clone(),
            guard.ext_popup_id.as_deref() == Some(id),
            guard.private || !guard.workspace.is_empty(),
        )
    };
    hide_extension_popup(shell);
    // Private windows and other workspaces don't run extensions.
    let (Some(window), false, false) = (window, same, private) else {
        return;
    };
    let Some(url) = extension_popup_url(id) else {
        return;
    };
    // Marked as showing before its browser exists: the browser can start while it's being
    // made, and its start-up looks for this to know it's still wanted.
    lock(shell).ext_popup_id = Some(id.to_owned());
    let mut client = ExtensionPopupClient::new(shell.clone(), id.to_owned());
    let Some(view) = browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url.as_str())),
        Some(&BrowserSettings::default()),
        None,
        None,
        Some(&mut ExtensionPopupViewHost::new()),
    ) else {
        release_extension_popup(shell);
        return;
    };
    let Some(overlay) =
        window.add_overlay_view(Some(&mut View::from(&view)), DockingMode::CUSTOM, 0)
    else {
        release_extension_popup(shell);
        return;
    };
    overlay.set_bounds(Some(&Rect {
        x: left,
        y: top,
        width: EXTENSION_POPUP_WIDTH,
        height: EXTENSION_POPUP_HEIGHT,
    }));
    overlay.set_visible(1);
    View::from(&view).request_focus();
    let mut guard = lock(shell);
    guard.ext_popup_view = Some(view);
    guard.ext_popup_overlay = Some(overlay);
}

/// Puts the popup away. Its browser finishes closing once nothing holds its view.
fn hide_extension_popup(shell: &SharedShell) {
    let (overlay, browser) = {
        let mut guard = lock(shell);
        if guard.ext_popup_id.take().is_none() {
            return;
        }
        (
            guard.ext_popup_overlay.clone(),
            guard.ext_popup_browser.clone(),
        )
    };
    if let Some(overlay) = overlay {
        overlay.set_visible(0);
    }
    match browser.and_then(|browser| browser.host()) {
        Some(host) => {
            host.close_browser(0);
        }
        // It hasn't finished starting, so there's nothing to close yet. Let go of it.
        None => release_extension_popup(shell),
    }
}

/// Lets go of what holds the popup, which lets its browser finish closing.
fn release_extension_popup(shell: &SharedShell) {
    let (overlay, view) = {
        let mut guard = lock(shell);
        guard.ext_popup_id = None;
        guard.ext_popup_browser = None;
        (guard.ext_popup_overlay.take(), guard.ext_popup_view.take())
    };
    if let Some(overlay) = overlay {
        overlay.destroy();
    }
    drop(view);
}

wrap_browser_view_delegate! {
    struct ExtensionPopupViewHost {}

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

    }
}

wrap_keyboard_handler! {
    struct ExtensionPopupKeys {
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
            popup_escape(&self.shell, event)
        }

        #[cfg(target_os = "macos")]
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut i32>,
        ) -> i32 {
            popup_escape(&self.shell, event)
        }
    }
}

/// Escape puts the popup away. Every other key stays with the popup's page.
fn popup_escape(shell: &SharedShell, event: Option<&KeyEvent>) -> i32 {
    const ESCAPE: i32 = 0x1B;
    let is_escape = event.is_some_and(|event| {
        event.type_ == KeyEventType::RAWKEYDOWN && event.windows_key_code == ESCAPE
    });
    if is_escape {
        hide_extension_popup(shell);
    }
    is_escape.into()
}

wrap_request_handler! {
    struct ExtensionPopupRequests {
        id: String,
    }

    impl RequestHandler {
        // A popup shows its own extension's pages and nothing else, so a link in it
        // can't turn the popup into a window on the web.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let own = format!("chrome-extension://{}/", self.id);
            let url = request.map(|request| CefString::from(&request.url()).to_string());
            (!url.is_some_and(|url| url.starts_with(&own))).into()
        }
    }
}

wrap_client! {
    struct ExtensionPopupClient {
        shell: SharedShell,
        id: String,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ExtensionPopupLifeSpan::new(self.shell.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(ExtensionPopupKeys::new(self.shell.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(ExtensionPopupRequests::new(self.id.clone()))
        }
    }
}

wrap_life_span_handler! {
    struct ExtensionPopupLifeSpan {
        shell: SharedShell,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let wanted = {
                let mut shell = lock(&self.shell);
                shell.open_browsers += 1;
                let wanted = shell.ext_popup_id.is_some();
                if wanted {
                    shell.ext_popup_browser = browser.as_deref().cloned();
                }
                wanted
            };
            // The person may have put the popup away before it finished starting. Closing
            // is done without holding the lock, because it calls back into this code.
            if !wanted && let Some(host) = browser.as_deref().and_then(|browser| browser.host()) {
                host.close_browser(0);
            }
        }

        // A popup can't open windows of its own.
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: i32,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut i32>,
        ) -> i32 {
            1
        }

        // Like a tab, the popup's view lives in the window, which must stay open when it
        // closes. Letting go of the view is what lets the browser finish.
        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            if lock(&self.shell).window_closing {
                return 0;
            }
            release_extension_popup(&self.shell);
            1
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            browser_closed(&self.shell, Some(TabId::MAX), None);
        }
    }
}

// -- Page icons --------------------------------------------------------------------------

fn send_favicon(shell: &SharedShell, url: &str, data: &str) {
    // One lock for both reads: locking again while this guard lives would never return.
    let toolbar = {
        let guard = lock(shell);
        guard.toolbar.clone().filter(|_| guard.toolbar_ready)
    };
    let event = UiEvent::Favicon { url, data };
    send_to_toolbar(toolbar.as_ref(), &event);
    send_to_toolbar(sidebar_of(shell).as_ref(), &event);
}

/// A toolbar that has just started needs the icons this window already has.
fn send_known_favicons(shell: &SharedShell) {
    let known: Vec<(String, String)> = lock(shell)
        .favicons
        .iter()
        .map(|(url, data)| (url.clone(), data.clone()))
        .collect();
    for (url, data) in known {
        send_favicon(shell, &url, &data);
    }
}

/// Fetches a page's icon once, and sends it to the toolbar.
fn request_favicon(shell: &SharedShell, host: &BrowserHost, url: String) {
    let known = {
        let mut guard = lock(shell);
        if let Some(data) = guard.favicons.get(&url) {
            Some(data.clone())
        } else if guard.favicons_loading.len() >= 16 || !guard.favicons_loading.insert(url.clone())
        {
            return;
        } else {
            None
        }
    };
    if let Some(data) = known {
        send_favicon(shell, &url, &data);
        return;
    }
    let mut callback = FaviconDownloaded::new(shell.clone(), url.clone());
    host.download_image(
        Some(&CefString::from(url.as_str())),
        1,
        32,
        0,
        Some(&mut callback),
    );
}

wrap_download_image_callback! {
    struct FaviconDownloaded {
        shell: SharedShell,
        url: String,
    }

    impl DownloadImageCallback {
        fn on_download_image_finished(
            &self,
            _image_url: Option<&CefString>,
            _http_status_code: i32,
            image: Option<&mut Image>,
        ) {
            lock(&self.shell).favicons_loading.remove(&self.url);
            // An image has a picture for each screen density it was fetched for, and none
            // for the others, so ask for the ones a screen can have.
            // An image has a picture for each screen density it was fetched for, and none
            // for the others, so ask for the ones a screen can have. CEF returns nothing
            // unless it's given somewhere to write the picture's size.
            let png = image.and_then(|image| {
                let (mut width, mut height) = (0, 0);
                [2.0, 1.0, 3.0].into_iter().find_map(|scale| {
                    image.as_png(scale, 1, Some(&mut width), Some(&mut height))
                })
            });
            let Some(png) = png else {
                return;
            };
            let mut bytes = vec![0u8; png.size()];
            png.data(Some(&mut bytes), 0);
            // A huge icon would be sent to the toolbar and kept for nothing.
            if bytes.is_empty() || bytes.len() > FAVICON_LIMIT_BYTES {
                return;
            }
            let data = format!("data:image/png;base64,{}", BASE64.encode(&bytes));
            lock(&self.shell).favicons.insert(self.url.clone(), data.clone());
            send_favicon(&self.shell, &self.url, &data);
        }
    }
}
