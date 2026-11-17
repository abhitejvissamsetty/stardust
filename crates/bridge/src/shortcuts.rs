#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    FocusAddress,
    Back,
    Forward,
    Reload,
    HardReload,
    Stop,
    NewTab,
    NewWindow,
    CloseTab,
    Quit,
    NextTab,
    PreviousTab,
    /// A zero-based tab position.
    TabAt(usize),
    LastTab,
    /// Moves the active tab one place left or right.
    MoveTabLeft,
    MoveTabRight,
    Minimize,
    ToggleFullscreen,
    NewPrivateWindow,
    ReopenClosedTab,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Print,
    DevTools,
    Find,
    FindNext,
    FindPrevious,
    ToggleBookmark,
    History,
    Bookmarks,
    Settings,
    Downloads,
    ToggleSidePanel,
    /// Chrome calls this "Search tabs"; here it also finds workspaces and commands.
    CommandBar,
    /// Saves the page to the side panel's clips.
    ClipPage,
}

/// A key press, using Windows virtual key codes as CEF reports them on every platform.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyPress {
    pub key_code: i32,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub command: bool,
}

const TAB: i32 = 0x09;
const ESCAPE: i32 = 0x1B;
const KEY_1: i32 = 0x31;
const KEY_8: i32 = 0x38;
const KEY_9: i32 = 0x39;
const T: i32 = 0x54;
const LEFT: i32 = 0x25;
const RIGHT: i32 = 0x27;
const A: i32 = 0x41;
const D: i32 = 0x44;
const E: i32 = 0x45;
const F: i32 = 0x46;
const G: i32 = 0x47;
const H: i32 = 0x48;
const F3: i32 = 0x72;
const J: i32 = 0x4A;
const L: i32 = 0x4C;
const N: i32 = 0x4E;
const O: i32 = 0x4F;
const COMMA: i32 = 0xBC;
const Q: i32 = 0x51;
const R: i32 = 0x52;
const W: i32 = 0x57;
const F5: i32 = 0x74;
const F6: i32 = 0x75;
const F11: i32 = 0x7A;
const F12: i32 = 0x7B;
const I: i32 = 0x49;
const P: i32 = 0x50;
const S: i32 = 0x53;
const KEY_0: i32 = 0x30;
const PLUS: i32 = 0xBB;
const MINUS: i32 = 0xBD;
const NUMPAD_ADD: i32 = 0x6B;
const NUMPAD_SUBTRACT: i32 = 0x6D;
const M: i32 = 0x4D;
const PAGE_UP: i32 = 0x21;
const PAGE_DOWN: i32 = 0x22;
const OPEN_BRACKET: i32 = 0xDB;
const CLOSE_BRACKET: i32 = 0xDD;

/// Finds the browser shortcut for a key press. `is_mac` swaps Ctrl for Command.
pub fn shortcut_for(key: KeyPress, is_mac: bool) -> Option<Shortcut> {
    let primary = if is_mac { key.command } else { key.ctrl };
    let other = if is_mac { key.ctrl } else { key.command };
    // Ctrl+Cmd+F is macOS's full screen shortcut, the one Ctrl combination that isn't Tab.
    let is_mac_full_screen = is_mac && key.ctrl && key.command && key.key_code == F;
    if other && key.key_code != TAB && !is_mac_full_screen {
        return None;
    }
    if is_mac_full_screen {
        return Some(Shortcut::ToggleFullscreen);
    }

    match (key.key_code, primary, key.shift, key.alt) {
        (L, true, false, false) | (F6, false, false, false) => Some(Shortcut::FocusAddress),
        (D, false, false, true) if !is_mac => Some(Shortcut::FocusAddress),
        (R, true, false, false) | (F5, false, false, false) => Some(Shortcut::Reload),
        (R, true, true, false) | (F5, false, true, false) => Some(Shortcut::HardReload),
        (LEFT, false, false, true) if !is_mac => Some(Shortcut::Back),
        (RIGHT, false, false, true) if !is_mac => Some(Shortcut::Forward),
        (OPEN_BRACKET | LEFT, true, false, false) if is_mac => Some(Shortcut::Back),
        (CLOSE_BRACKET | RIGHT, true, false, false) if is_mac => Some(Shortcut::Forward),
        (ESCAPE, false, false, false) => Some(Shortcut::Stop),
        (F, true, false, false) => Some(Shortcut::Find),
        (G, true, false, false) | (F3, false, false, false) => Some(Shortcut::FindNext),
        (G, true, true, false) | (F3, false, true, false) => Some(Shortcut::FindPrevious),
        (D, true, false, false) => Some(Shortcut::ToggleBookmark),
        (H, true, false, false) => Some(Shortcut::History),
        (J, true, false, false) => Some(Shortcut::Downloads),
        (O, true, true, false) => Some(Shortcut::Bookmarks),
        (COMMA, true, false, false) => Some(Shortcut::Settings),
        (T, true, false, false) => Some(Shortcut::NewTab),
        (N, true, false, false) => Some(Shortcut::NewWindow),
        (W, true, false, false) => Some(Shortcut::CloseTab),
        (Q, true, false, false) => Some(Shortcut::Quit),
        // Ctrl+Tab on every platform, including macOS.
        (TAB, _, false, false) if key.ctrl => Some(Shortcut::NextTab),
        (TAB, _, true, false) if key.ctrl => Some(Shortcut::PreviousTab),
        (KEY_1..=KEY_8, true, false, false) => {
            Some(Shortcut::TabAt((key.key_code - KEY_1) as usize))
        }
        (KEY_9, true, false, false) => Some(Shortcut::LastTab),
        (F11, false, false, false) => Some(Shortcut::ToggleFullscreen),
        (F12, false, false, false) => Some(Shortcut::DevTools),
        // Chrome: Cmd+Option+I on macOS, Ctrl+Shift+I elsewhere.
        (I, true, false, true) if is_mac => Some(Shortcut::DevTools),
        (I, true, true, false) if !is_mac => Some(Shortcut::DevTools),
        // Zoom: + may be typed with or without Shift.
        (PLUS | NUMPAD_ADD, true, _, false) => Some(Shortcut::ZoomIn),
        (MINUS | NUMPAD_SUBTRACT, true, false, false) => Some(Shortcut::ZoomOut),
        (KEY_0, true, false, false) => Some(Shortcut::ZoomReset),
        (P, true, false, false) => Some(Shortcut::Print),
        (N, true, true, false) => Some(Shortcut::NewPrivateWindow),
        (E, true, true, false) => Some(Shortcut::ToggleSidePanel),
        (A, true, true, false) => Some(Shortcut::CommandBar),
        (S, true, true, false) => Some(Shortcut::ClipPage),
        (T, true, true, false) => Some(Shortcut::ReopenClosedTab),
        // Chrome on macOS: Cmd+M minimizes, and Ctrl+Cmd+F toggles full screen.
        (M, true, false, false) if is_mac => Some(Shortcut::Minimize),
        (PAGE_UP, true, true, false) => Some(Shortcut::MoveTabLeft),
        (PAGE_DOWN, true, true, false) => Some(Shortcut::MoveTabRight),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key_code: i32) -> KeyPress {
        KeyPress {
            key_code,
            ..Default::default()
        }
    }

    #[test]
    fn page_and_window_shortcuts_like_chrome() {
        let cmd = |key_code, shift, alt| KeyPress {
            command: true,
            shift,
            alt,
            ..press(key_code)
        };
        let ctrl = |key_code, shift| KeyPress {
            ctrl: true,
            shift,
            ..press(key_code)
        };
        assert_eq!(
            shortcut_for(cmd(PLUS, false, false), true),
            Some(Shortcut::ZoomIn)
        );
        assert_eq!(
            shortcut_for(cmd(PLUS, true, false), true),
            Some(Shortcut::ZoomIn)
        );
        assert_eq!(
            shortcut_for(cmd(MINUS, false, false), true),
            Some(Shortcut::ZoomOut)
        );
        assert_eq!(
            shortcut_for(cmd(KEY_0, false, false), true),
            Some(Shortcut::ZoomReset)
        );
        assert_eq!(
            shortcut_for(cmd(P, false, false), true),
            Some(Shortcut::Print)
        );
        assert_eq!(
            shortcut_for(cmd(N, true, false), true),
            Some(Shortcut::NewPrivateWindow)
        );
        assert_eq!(
            shortcut_for(cmd(E, true, false), true),
            Some(Shortcut::ToggleSidePanel)
        );
        assert_eq!(
            shortcut_for(ctrl(E, true), false),
            Some(Shortcut::ToggleSidePanel)
        );
        assert_eq!(
            shortcut_for(cmd(A, true, false), true),
            Some(Shortcut::CommandBar)
        );
        assert_eq!(
            shortcut_for(ctrl(A, true), false),
            Some(Shortcut::CommandBar)
        );
        assert_eq!(
            shortcut_for(cmd(S, true, false), true),
            Some(Shortcut::ClipPage)
        );
        assert_eq!(shortcut_for(cmd(S, false, false), true), None);
        // Without Shift, the key isn't taken.
        assert_eq!(shortcut_for(cmd(E, false, false), true), None);
        assert_eq!(
            shortcut_for(cmd(T, true, false), true),
            Some(Shortcut::ReopenClosedTab)
        );
        assert_eq!(
            shortcut_for(cmd(I, false, true), true),
            Some(Shortcut::DevTools)
        );
        assert_eq!(shortcut_for(ctrl(I, true), false), Some(Shortcut::DevTools));
        assert_eq!(shortcut_for(press(F12), false), Some(Shortcut::DevTools));
        // Plain new window and new tab are unchanged.
        assert_eq!(
            shortcut_for(cmd(N, false, false), true),
            Some(Shortcut::NewWindow)
        );
        assert_eq!(shortcut_for(ctrl(T, false), false), Some(Shortcut::NewTab));
    }

    #[test]
    fn window_shortcuts() {
        let key = |key_code, command, ctrl| KeyPress {
            command,
            ctrl,
            ..press(key_code)
        };
        assert_eq!(
            shortcut_for(press(F11), false),
            Some(Shortcut::ToggleFullscreen)
        );
        assert_eq!(
            shortcut_for(key(M, true, false), true),
            Some(Shortcut::Minimize)
        );
        assert_eq!(
            shortcut_for(key(F, true, true), true),
            Some(Shortcut::ToggleFullscreen)
        );
        // Plain Cmd+F is still find, and Ctrl+M means nothing on Windows or Linux.
        assert_eq!(
            shortcut_for(key(F, true, false), true),
            Some(Shortcut::Find)
        );
        assert_eq!(shortcut_for(key(M, false, true), false), None);
    }

    #[test]
    fn moving_tabs_by_keyboard() {
        let ctrl_shift = |key_code| KeyPress {
            ctrl: true,
            shift: true,
            ..press(key_code)
        };
        assert_eq!(
            shortcut_for(ctrl_shift(PAGE_UP), false),
            Some(Shortcut::MoveTabLeft)
        );
        assert_eq!(
            shortcut_for(ctrl_shift(PAGE_DOWN), false),
            Some(Shortcut::MoveTabRight)
        );
        let command_shift = KeyPress {
            command: true,
            shift: true,
            ..press(PAGE_DOWN)
        };
        assert_eq!(
            shortcut_for(command_shift, true),
            Some(Shortcut::MoveTabRight)
        );
    }

    #[test]
    fn linux_and_windows_shortcuts() {
        let ctrl_l = KeyPress {
            ctrl: true,
            ..press(L)
        };
        assert_eq!(shortcut_for(ctrl_l, false), Some(Shortcut::FocusAddress));
        let alt_left = KeyPress {
            alt: true,
            ..press(LEFT)
        };
        assert_eq!(shortcut_for(alt_left, false), Some(Shortcut::Back));
        let ctrl_shift_r = KeyPress {
            ctrl: true,
            shift: true,
            ..press(R)
        };
        assert_eq!(
            shortcut_for(ctrl_shift_r, false),
            Some(Shortcut::HardReload)
        );
        assert_eq!(shortcut_for(press(F5), false), Some(Shortcut::Reload));
    }

    #[test]
    fn mac_uses_command() {
        let command_l = KeyPress {
            command: true,
            ..press(L)
        };
        assert_eq!(shortcut_for(command_l, true), Some(Shortcut::FocusAddress));
        let ctrl_l = KeyPress {
            ctrl: true,
            ..press(L)
        };
        assert_eq!(shortcut_for(ctrl_l, true), None);
        let command_bracket = KeyPress {
            command: true,
            ..press(OPEN_BRACKET)
        };
        assert_eq!(shortcut_for(command_bracket, true), Some(Shortcut::Back));
    }

    #[test]
    fn tab_shortcuts() {
        let ctrl = |key_code| KeyPress {
            ctrl: true,
            ..press(key_code)
        };
        assert_eq!(shortcut_for(ctrl(T), false), Some(Shortcut::NewTab));
        assert_eq!(shortcut_for(ctrl(N), false), Some(Shortcut::NewWindow));
        assert_eq!(shortcut_for(ctrl(W), false), Some(Shortcut::CloseTab));
        assert_eq!(shortcut_for(ctrl(Q), false), Some(Shortcut::Quit));
        assert_eq!(shortcut_for(ctrl(TAB), false), Some(Shortcut::NextTab));
        assert_eq!(shortcut_for(ctrl(TAB), true), Some(Shortcut::NextTab));
        let ctrl_shift_tab = KeyPress {
            shift: true,
            ..ctrl(TAB)
        };
        assert_eq!(
            shortcut_for(ctrl_shift_tab, false),
            Some(Shortcut::PreviousTab)
        );
        assert_eq!(shortcut_for(ctrl(KEY_1), false), Some(Shortcut::TabAt(0)));
        assert_eq!(shortcut_for(ctrl(KEY_8), false), Some(Shortcut::TabAt(7)));
        assert_eq!(shortcut_for(ctrl(KEY_9), false), Some(Shortcut::LastTab));
        assert_eq!(shortcut_for(press(TAB), false), None);
    }

    #[test]
    fn find_shortcuts() {
        let ctrl = |key_code| KeyPress {
            ctrl: true,
            ..press(key_code)
        };
        assert_eq!(shortcut_for(ctrl(F), false), Some(Shortcut::Find));
        assert_eq!(shortcut_for(press(F3), false), Some(Shortcut::FindNext));
        let shift_f3 = KeyPress {
            shift: true,
            ..press(F3)
        };
        assert_eq!(shortcut_for(shift_f3, false), Some(Shortcut::FindPrevious));
        assert_eq!(shortcut_for(ctrl(G), false), Some(Shortcut::FindNext));
    }

    #[test]
    fn data_page_shortcuts() {
        let ctrl = |key_code| KeyPress {
            ctrl: true,
            ..press(key_code)
        };
        assert_eq!(shortcut_for(ctrl(D), false), Some(Shortcut::ToggleBookmark));
        assert_eq!(shortcut_for(ctrl(H), false), Some(Shortcut::History));
        assert_eq!(shortcut_for(ctrl(COMMA), false), Some(Shortcut::Settings));
        let ctrl_shift_o = KeyPress {
            shift: true,
            ..ctrl(O)
        };
        assert_eq!(shortcut_for(ctrl_shift_o, false), Some(Shortcut::Bookmarks));
    }

    #[test]
    fn plain_typing_is_left_alone() {
        assert_eq!(shortcut_for(press(L), false), None);
        assert_eq!(shortcut_for(press(LEFT), false), None);
        let ctrl_alt_l = KeyPress {
            ctrl: true,
            alt: true,
            ..press(L)
        };
        assert_eq!(shortcut_for(ctrl_alt_l, false), None);
    }
}
