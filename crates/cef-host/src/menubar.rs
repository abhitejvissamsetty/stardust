//! The macOS menu bar, laid out like Chrome's: Stardust, File, Edit, View, History,
//! Bookmarks, Profiles, Tab, Window and Help.
//!
//! Editing, hiding, minimizing and quitting go through Cocoa's own responder chain, so
//! they act on whatever has focus. Everything else is one of `MenuAction`, run by the
//! browser on the window that's in front.

use std::cell::RefCell;

use bridge::Shortcut;
use objc2::{
    MainThreadMarker, MainThreadOnly, class, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, NSObjectProtocol, Sel},
};

/// What a menu item does, apart from the editing and window commands Cocoa handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    Shortcut(Shortcut),
    CloseWindow,
    ViewSource,
    DuplicateTab,
    CloseOtherTabs,
    CloseTabsToTheRight,
    MuteTab,
    BookmarkAllTabs,
    SavePage,
    OpenFile,
    ToggleSplitView,
    ToggleVerticalTabs,
    Screenshot { full: bool },
    PickColor,
    InspectText,
    Library,
    Welcome,
    Extensions,
    Workspace(String),
    ManageWorkspaces,
}

const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
/// Tags from here up are workspaces in the Profiles menu.
const PROFILE_TAG: isize = 100_000;

enum Entry {
    Separator,
    /// A command the browser runs.
    Action(&'static str, &'static str, usize, MenuAction),
    /// A command Cocoa runs on whatever has focus, named by its selector.
    Cocoa(&'static str, &'static str, usize, &'static std::ffi::CStr),
    Submenu(&'static str, Vec<Entry>),
}

use Entry::{Action, Cocoa, Separator, Submenu};
use MenuAction::{Shortcut as Sc, *};

thread_local! {
    static ACTIONS: RefCell<Vec<MenuAction>> = const { RefCell::new(Vec::new()) };
    static PROFILE_ACTIONS: RefCell<Vec<MenuAction>> = const { RefCell::new(Vec::new()) };
    static TARGET: RefCell<Option<Retained<MenuTarget>>> = const { RefCell::new(None) };
    static PROFILES: RefCell<Option<*mut AnyObject>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "StardustMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(invoke:))]
        fn invoke(&self, sender: &AnyObject) {
            let tag: isize = unsafe { msg_send![sender, tag] };
            let action = if tag >= PROFILE_TAG {
                PROFILE_ACTIONS.with(|list| list.borrow().get((tag - PROFILE_TAG) as usize).cloned())
            } else {
                ACTIONS.with(|list| list.borrow().get(tag as usize).cloned())
            };
            if let Some(action) = action {
                crate::client::run_menu_action(action);
            }
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

fn text(value: &str) -> *mut AnyObject {
    let value = std::ffi::CString::new(value.replace('\0', "")).unwrap_or_default();
    unsafe { msg_send![class!(NSString), stringWithUTF8String: value.as_ptr()] }
}

unsafe fn new_menu(title: &str) -> *mut AnyObject {
    let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
    let menu: *mut AnyObject = msg_send![menu, initWithTitle: text(title)];
    // Items are shown as they are: a menu that's switched off by validation would grey
    // out everything that isn't wired through the responder chain.
    let _: () = msg_send![menu, setAutoenablesItems: false];
    menu
}

unsafe fn add_item(
    menu: *mut AnyObject,
    title: &str,
    key: &str,
    mask: usize,
    action: Sel,
    target: Option<&MenuTarget>,
    tag: isize,
) -> *mut AnyObject {
    let item: *mut AnyObject = msg_send![class!(NSMenuItem), alloc];
    let item: *mut AnyObject = msg_send![
        item,
        initWithTitle: text(title),
        action: action,
        keyEquivalent: text(key)
    ];
    let _: () = msg_send![item, setKeyEquivalentModifierMask: mask];
    let _: () = msg_send![item, setTag: tag];
    if let Some(target) = target {
        let _: () = msg_send![item, setTarget: target];
    }
    let _: () = msg_send![menu, addItem: item];
    item
}

fn build(menu: *mut AnyObject, entries: Vec<Entry>, target: &MenuTarget) {
    for entry in entries {
        unsafe {
            match entry {
                Separator => {
                    let item: *mut AnyObject = msg_send![class!(NSMenuItem), separatorItem];
                    let _: () = msg_send![menu, addItem: item];
                }
                Action(title, key, mask, action) => {
                    let tag = ACTIONS.with(|list| {
                        let mut list = list.borrow_mut();
                        list.push(action);
                        list.len() as isize - 1
                    });
                    add_item(
                        menu,
                        title,
                        key,
                        mask,
                        objc2::sel!(invoke:),
                        Some(target),
                        tag,
                    );
                }
                Cocoa(title, key, mask, selector) => {
                    add_item(menu, title, key, mask, Sel::register(selector), None, 0);
                }
                Submenu(title, entries) => {
                    let sub = new_menu(title);
                    build(sub, entries, target);
                    let item = add_item(menu, title, "", 0, Sel::register(c""), None, 0);
                    let _: () = msg_send![item, setSubmenu: sub];
                }
            }
        }
    }
}

fn app_menu() -> Vec<Entry> {
    vec![
        Cocoa("About Stardust", "", 0, c"orderFrontStandardAboutPanel:"),
        Separator,
        Action("Settings…", ",", COMMAND, Sc(Shortcut::Settings)),
        Separator,
        Cocoa("Hide Stardust", "h", COMMAND, c"hide:"),
        Cocoa(
            "Hide Others",
            "h",
            COMMAND | OPTION,
            c"hideOtherApplications:",
        ),
        Cocoa("Show All", "", 0, c"unhideAllApplications:"),
        Separator,
        Cocoa("Quit Stardust", "q", COMMAND, c"terminate:"),
    ]
}

fn file_menu() -> Vec<Entry> {
    vec![
        Action("New Tab", "t", COMMAND, Sc(Shortcut::NewTab)),
        Action("New Window", "n", COMMAND, Sc(Shortcut::NewWindow)),
        Action(
            "New Private Window",
            "n",
            COMMAND | SHIFT,
            Sc(Shortcut::NewPrivateWindow),
        ),
        Action("Open File…", "o", COMMAND, OpenFile),
        Action("Open Location…", "l", COMMAND, Sc(Shortcut::FocusAddress)),
        Separator,
        Action("Close Window", "w", COMMAND | SHIFT, CloseWindow),
        Action("Close Tab", "w", COMMAND, Sc(Shortcut::CloseTab)),
        Action("Save Page As…", "s", COMMAND, SavePage),
        Separator,
        Action("Print…", "p", COMMAND, Sc(Shortcut::Print)),
        Separator,
        Action(
            "Take Screenshot of What's Showing",
            "",
            0,
            Screenshot { full: false },
        ),
        Action(
            "Take Screenshot of the Whole Page",
            "",
            0,
            Screenshot { full: true },
        ),
    ]
}

fn edit_menu() -> Vec<Entry> {
    vec![
        Cocoa("Undo", "z", COMMAND, c"undo:"),
        Cocoa("Redo", "z", COMMAND | SHIFT, c"redo:"),
        Separator,
        Cocoa("Cut", "x", COMMAND, c"cut:"),
        Cocoa("Copy", "c", COMMAND, c"copy:"),
        Cocoa("Paste", "v", COMMAND, c"paste:"),
        Cocoa(
            "Paste and Match Style",
            "v",
            COMMAND | OPTION | SHIFT,
            c"pasteAndMatchStyle:",
        ),
        Cocoa("Delete", "", 0, c"delete:"),
        Cocoa("Select All", "a", COMMAND, c"selectAll:"),
        Separator,
        Submenu(
            "Find",
            vec![
                Action("Find…", "f", COMMAND, Sc(Shortcut::Find)),
                Action("Find Next", "g", COMMAND, Sc(Shortcut::FindNext)),
                Action(
                    "Find Previous",
                    "g",
                    COMMAND | SHIFT,
                    Sc(Shortcut::FindPrevious),
                ),
            ],
        ),
        Separator,
        Action(
            "Search Tabs and Commands…",
            "a",
            COMMAND | SHIFT,
            Sc(Shortcut::CommandBar),
        ),
    ]
}

fn view_menu() -> Vec<Entry> {
    vec![
        Action("Stop", ".", COMMAND, Sc(Shortcut::Stop)),
        Action("Reload This Page", "r", COMMAND, Sc(Shortcut::Reload)),
        Action(
            "Force Reload This Page",
            "r",
            COMMAND | SHIFT,
            Sc(Shortcut::HardReload),
        ),
        Separator,
        Action(
            "Enter Full Screen",
            "f",
            COMMAND | CONTROL,
            Sc(Shortcut::ToggleFullscreen),
        ),
        Separator,
        Action("Actual Size", "0", COMMAND, Sc(Shortcut::ZoomReset)),
        Action("Zoom In", "+", COMMAND, Sc(Shortcut::ZoomIn)),
        Action("Zoom Out", "-", COMMAND, Sc(Shortcut::ZoomOut)),
        Separator,
        Action("Split View", "", 0, ToggleSplitView),
        Action(
            "Side Panel",
            "e",
            COMMAND | SHIFT,
            Sc(Shortcut::ToggleSidePanel),
        ),
        Action("Show Tabs Down the Side", "", 0, ToggleVerticalTabs),
        Separator,
        Submenu(
            "Developer",
            vec![
                Action("View Source", "u", COMMAND | OPTION, ViewSource),
                Action(
                    "Developer Tools",
                    "i",
                    COMMAND | OPTION,
                    Sc(Shortcut::DevTools),
                ),
                Action("Pick a Colour From the Page", "", 0, PickColor),
                Action("Identify Fonts and Colours of Text", "", 0, InspectText),
            ],
        ),
    ]
}

fn history_menu() -> Vec<Entry> {
    vec![
        Action("Back", "[", COMMAND, Sc(Shortcut::Back)),
        Action("Forward", "]", COMMAND, Sc(Shortcut::Forward)),
        Separator,
        Action(
            "Reopen Closed Tab",
            "t",
            COMMAND | SHIFT,
            Sc(Shortcut::ReopenClosedTab),
        ),
        Separator,
        Action("Show Full History", "y", COMMAND, Sc(Shortcut::History)),
    ]
}

fn bookmarks_menu() -> Vec<Entry> {
    vec![
        Action(
            "Bookmark This Tab…",
            "d",
            COMMAND,
            Sc(Shortcut::ToggleBookmark),
        ),
        Action("Bookmark All Tabs…", "d", COMMAND | SHIFT, BookmarkAllTabs),
        Separator,
        Action(
            "Bookmark Manager",
            "o",
            COMMAND | OPTION,
            Sc(Shortcut::Bookmarks),
        ),
        Separator,
        Action(
            "Clip This Page",
            "s",
            COMMAND | SHIFT,
            Sc(Shortcut::ClipPage),
        ),
        Action("Library", "", 0, Library),
    ]
}

fn tab_menu() -> Vec<Entry> {
    let mut entries = vec![
        Action("New Tab", "t", COMMAND, Sc(Shortcut::NewTab)),
        Action("Duplicate Tab", "", 0, DuplicateTab),
        Action("Mute Site", "", 0, MuteTab),
        Separator,
        Action("Close Tab", "w", COMMAND, Sc(Shortcut::CloseTab)),
        Action("Close Other Tabs", "", 0, CloseOtherTabs),
        Action("Close Tabs to the Right", "", 0, CloseTabsToTheRight),
        Separator,
        Action("Select Next Tab", "\t", CONTROL, Sc(Shortcut::NextTab)),
        Action(
            "Select Previous Tab",
            "\t",
            CONTROL | SHIFT,
            Sc(Shortcut::PreviousTab),
        ),
        Action("Move Tab Left", "", 0, Sc(Shortcut::MoveTabLeft)),
        Action("Move Tab Right", "", 0, Sc(Shortcut::MoveTabRight)),
        Separator,
    ];
    for (number, key) in ["1", "2", "3", "4", "5", "6", "7", "8"]
        .into_iter()
        .enumerate()
    {
        entries.push(Action(
            [
                "Select Tab 1",
                "Select Tab 2",
                "Select Tab 3",
                "Select Tab 4",
                "Select Tab 5",
                "Select Tab 6",
                "Select Tab 7",
                "Select Tab 8",
            ][number],
            key,
            COMMAND,
            Sc(Shortcut::TabAt(number)),
        ));
    }
    entries.push(Action(
        "Select Last Tab",
        "9",
        COMMAND,
        Sc(Shortcut::LastTab),
    ));
    entries
}

fn window_menu() -> Vec<Entry> {
    vec![
        Cocoa("Minimize", "m", COMMAND, c"performMiniaturize:"),
        Cocoa("Zoom", "", 0, c"performZoom:"),
        Separator,
        Action("Downloads", "j", COMMAND | SHIFT, Sc(Shortcut::Downloads)),
        Action("Extensions", "", 0, Extensions),
        Separator,
        Cocoa("Bring All to Front", "", 0, c"arrangeInFront:"),
    ]
}

fn help_menu() -> Vec<Entry> {
    vec![
        Action("Welcome to Stardust", "", 0, Welcome),
        Separator,
        Action("Search Tabs and Commands…", "", 0, Sc(Shortcut::CommandBar)),
    ]
}

/// Installs the menu bar. Call once, on the main thread, after CEF has started.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let target: Retained<MenuTarget> = {
        let this = mtm.alloc::<MenuTarget>().set_ivars(());
        unsafe { msg_send![super(this), init] }
    };
    ACTIONS.with(|list| list.borrow_mut().clear());
    unsafe {
        let bar = new_menu("");
        let sections: [(&str, Vec<Entry>); 10] = [
            ("Stardust", app_menu()),
            ("File", file_menu()),
            ("Edit", edit_menu()),
            ("View", view_menu()),
            ("History", history_menu()),
            ("Bookmarks", bookmarks_menu()),
            ("Profiles", Vec::new()),
            ("Tab", tab_menu()),
            ("Window", window_menu()),
            ("Help", help_menu()),
        ];
        let mut windows = std::ptr::null_mut::<AnyObject>();
        let mut help = std::ptr::null_mut::<AnyObject>();
        for (title, entries) in sections {
            let sub = new_menu(title);
            build(sub, entries, &target);
            let item = add_item(bar, title, "", 0, Sel::register(c""), None, 0);
            let _: () = msg_send![item, setSubmenu: sub];
            match title {
                "Profiles" => PROFILES.with(|slot| *slot.borrow_mut() = Some(sub)),
                "Window" => windows = sub,
                "Help" => help = sub,
                _ => {}
            }
        }
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, setMainMenu: bar];
        if !windows.is_null() {
            let _: () = msg_send![app, setWindowsMenu: windows];
        }
        if !help.is_null() {
            let _: () = msg_send![app, setHelpMenu: help];
        }
    }
    TARGET.with(|slot| *slot.borrow_mut() = Some(target));
}

/// A workspace for the Profiles menu.
pub struct Profile {
    pub id: String,
    pub name: String,
    pub current: bool,
}

/// Rebuilds the Profiles menu: the workspaces, with a tick by the one this window uses.
pub fn set_profiles(profiles: Vec<Profile>) {
    let Some(menu) = PROFILES.with(|slot| *slot.borrow()) else {
        return;
    };
    let Some(target) = TARGET.with(|slot| slot.borrow().clone()) else {
        return;
    };
    let mut actions = Vec::new();
    unsafe {
        let _: () = msg_send![menu, removeAllItems];
        for profile in &profiles {
            let tag = PROFILE_TAG + actions.len() as isize;
            actions.push(Workspace(profile.id.clone()));
            let item = add_item(
                menu,
                &profile.name,
                "",
                0,
                objc2::sel!(invoke:),
                Some(&target),
                tag,
            );
            if profile.current {
                let _: () = msg_send![item, setState: 1isize];
            }
        }
        let separator: *mut AnyObject = msg_send![class!(NSMenuItem), separatorItem];
        let _: () = msg_send![menu, addItem: separator];
        let tag = PROFILE_TAG + actions.len() as isize;
        actions.push(ManageWorkspaces);
        add_item(
            menu,
            "Manage Workspaces…",
            "",
            0,
            objc2::sel!(invoke:),
            Some(&target),
            tag,
        );
    }
    PROFILE_ACTIONS.with(|list| *list.borrow_mut() = actions);
}
