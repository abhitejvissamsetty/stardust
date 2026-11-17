# Milestone 5: a Chrome-like interface, private windows and extensions

Built and tested on macOS (Apple Silicon) on 2026-11-17. Stardust's own pages (the toolbar, tab strip, menus, Settings and the extensions hub) were checked by screenshot and by driving them with real mouse and key events through Chromium's debugging protocol (`scripts/cdp.py`). The native window frame can't be seen that way, so what it does is checked from the window's reported state only.

## Status

| Piece | Status |
|---|---|
| Resize, maximize, minimize and macOS window buttons | **Fixed.** Maximize, restore and full screen confirmed. **Minimize wasn't confirmed** |
| Page icons on tabs | **Done** |
| Sound and mute buttons on tabs | **Done**, with a mute toggle tested. Real audio wasn't played |
| Unloaded tabs shown dimmed | **Done** |
| App menu (the three-dot button) | **Done** |
| Zoom, with a chip in the address bar | **Done** and tested |
| Reopen closed tab | **Done** and tested |
| Print, developer tools | **Added, not tested**: both open system windows |
| "Not secure" label on plain HTTP pages | **Done** |
| Private windows | **Done** and tested |
| Chrome extensions | **Done** for what Chromium allows here, tested end to end. See below |
| Extensions hub, toolbar menu, pinned buttons, popups | **Done** and tested |
| Settings regrouped into cards | **Done** and tested |
| Dark and private themes | **Done**, checked by screenshot |
| Installing from the Chrome Web Store | **Not built.** A decision for the owner, see below |

## Window controls

The window couldn't be resized, maximized or minimized, and on macOS it had no close, minimize or zoom buttons. CEF's own defaults are "yes" for all of these, but its Rust bindings default every answer to "no", and Stardust never overrode them. `crates/cef-host/src/app.rs` now answers yes to `can_resize`, `can_maximize`, `can_minimize` and `with_standard_window_buttons`.

The window keeps its native frame, so on every system its buttons look and behave as that system's windows do. Cmd+M (macOS), Ctrl+Cmd+F (macOS) and F11 minimize and toggle full screen. These were checked: maximize, restore, enter and leave full screen. **Minimize returned "not minimized" in every test**, and the cause isn't known: it may be how the test machine's window manager treats a window started from a script. Try the yellow button.

## Extensions

### What turned out to be true

An earlier note said Alloy-style CEF can't run Chrome extensions, because its API has no way to load them. That was right about the API, and wrong about the result. Chromium's extension system is still there underneath, and it loads any folder given to `--load-extension`. Tested with a purpose-built extension:

| Works | Doesn't |
|---|---|
| Background service workers | `tabs.query` and `windows.getAll` return nothing: there are no Chrome tabs or windows for them to find |
| Content scripts, which changed a page's title | Anything built around "the current tab" |
| Messages between content scripts and the background | Notifications and the omnibox |
| `storage`, `cookies`, `alarms` | The extension's toolbar button acting on a click, when it has no popup |
| Content blocking with `declarativeNetRequest`: a rule really blocked a request after a reload | |
| Extension pages, such as a popup, with real extension privileges | |

So content blockers, page changers, and extensions with their own pages work. Extensions that act on the tab you're looking at from a button, such as tab managers, mostly won't. The hub says so.

### How it works

- **Loading:** at startup, Stardust gives Chromium the folder of each enabled extension (`on_before_command_line_processing`). Chromium only loads extensions as it starts, so turning one on or off, adding or removing one needs a restart. The hub shows a "Restart now" prompt when what's loaded differs from what's saved, and the restart keeps the browser's original command-line options.
- **Where they live:** `Extensions/<random folder>` in Stardust's data folder, with the list in the settings table. The extension's ID is worked out the way Chromium does, from the folder's full path, and was checked against an ID Chromium really assigned.
- **Adding one:** Settings → Extensions, or `browser://extensions`, takes an unpacked folder, a `.zip` or a `.crx`. It unpacks into a staging folder, shows the extension's name, icon and what it can do in plain words, and only adds it when the person agrees.
- **The hub:** each extension has an on/off switch, details (permissions, sites, ID), pin to toolbar, and Remove, which asks twice. Anything the extension's author wrote is only ever set as text.
- **The toolbar:** a puzzle button opens a menu of the extensions that are on, each with a pin button, and a link to the hub. Pinned extensions show as buttons beside it. Clicking one with a popup shows the popup under the button (380×520, in a browser of its own). Pressing the button again, clicking elsewhere, switching tabs or pressing Escape puts it away.
- **Private windows** don't run extensions.

### Safety

- **Packages are untrusted.** Unpacking refuses files that point outside the folder, links, more than 20,000 files, and more than 200 MB unpacked. A failed unpack leaves nothing behind. Tested with a package containing `../evil.txt`.
- **Icons** may be PNG, JPEG, GIF or WebP, and are read by the browser and shown as `data:` addresses. SVG is refused because it can hold scripts.
- **The popup** is built from the browser's own saved record, never from an address sent by a page or the toolbar. It can only show its own extension's pages, can't open windows, and is for enabled extensions only.
- **What an extension can do** is described in plain words before it's added: for example "Read and change all your data on all websites" first, then cookies, tabs and so on. A strong example, `<all_urls>`, shows as the most sweeping line.
- **The picker stand-in** (`STARDUST_TEST_EXTENSION_PATH`) exists only in debug builds. The confirmation card still appears, so it skips the file picker and nothing else.

### Not done: the Chrome Web Store

Stardust can't yet install from the Chrome Web Store, which is where most people find extensions. Doing it means fetching extensions from Google's update service and unpacking the `.crx`. The unpacking is already built. **This is a decision for the owner,** because it's unclear that Google's terms allow a browser other than Chrome to use the store that way. Brave, Vivaldi and Edge do, but they have their own arrangements. Until that's decided, extensions come from files.

### Limits

- A new extension needs a restart before it runs.
- Extensions work on the web pages' content, not on Chrome's tabs and windows (above).
- A popup is a fixed size, not fitted to its content.
- A packaged `.crx` is unpacked without checking its signature, because the person picked the file themselves. Its ID then comes from its folder rather than the store's.
- No automatic extension updates, and no Web Store.
- Extensions that open windows or new tabs through the tabs API do nothing.

## Private windows

Cmd/Ctrl+Shift+N, or "New private window" in the menu. Each private window has a profile that lives in memory and goes with the window.

| In a private window | |
|---|---|
| Cookies and site storage | Kept in memory only. A normal window on the same site can't see them, and they never reach the cookie file. **Tested** |
| History | Not recorded. **Tested** |
| Open tabs | Not saved, so they don't come back after a restart. **Tested** |
| Downloads list | Not recorded. Files still save to disk |
| Closed-tab list | Not kept |
| Passwords | Not offered for saving. Saved ones can still be filled |
| Permission answers | Not remembered |
| Extensions | Not run |
| Bookmarks | Can be added, as in Chrome |
| Moving a tab | Can't cross between a private window and an ordinary one, either way |

A private window is always dark, with a purple accent and a "Private" badge, so it can't be mistaken for an ordinary one. Its frame's title ends in "— Private".

Not tested: closing the last private window and checking nothing remains; opening a private window while another is open (each has its own profile, so they don't share logins, unlike Chrome's).

## The interface

- **Tabs:** rounded, with the selected one joining the toolbar by a curve at each foot. Each shows the page's icon, which is fetched once and sent to the toolbar as a picture (not with every update). The spinner takes the icon's place while loading. A speaker or muted-speaker button shows while a tab plays sound, and clicking it mutes. Tabs that haven't loaded, or were unloaded to save memory, are dimmed.
- **Address bar:** a lock on secure pages, and an amber "Not secure" label on plain HTTP ones (not on local addresses, which are for development). A zoom chip appears when the page isn't at 100%, and resets it when clicked. The key button for saved logins and the star stay.
- **Toolbar:** the History, Bookmarks, Downloads and Settings buttons moved into the menu, leaving the extensions and menu buttons.
- **Menu:** New tab, New window, New private window; History, Downloads, Bookmarks, Extensions; zoom with full screen; Print, Find, Reopen closed tab, Developer tools; Settings; Quit. Each shows its shortcut for the system it's on.
- **Settings:** grouped into General, Downloads and performance, Privacy and security, Updates and Import, with proper switches.
- **Extensions hub and Settings** share card styles, and a new set of colours for warnings and success that work in both light and dark.
- **Shortcuts added:** Cmd/Ctrl+Shift+N (private window), Cmd/Ctrl+Shift+T (reopen tab), Cmd/Ctrl + `+`, `-`, `0` (zoom), Cmd/Ctrl+P (print), Cmd+Option+I or Ctrl+Shift+I or F12 (developer tools), Cmd+M and Ctrl+Cmd+F (macOS), F11.

## Bugs found while building

Both would have frozen the browser, and both were found by testing:

- **A double lock:** sending a page icon to the toolbar locked the window's state twice in one line, which waits forever. Fixed.
- **Closing while locked:** the extension popup's start-up called "close" while holding the window's lock, and closing calls back into code that needs the same lock. Fixed by marking the popup as open first, and never closing while locked.

One more thing found along the way: CEF's `as_png` quietly returns nothing unless it's given somewhere to write the picture's size, so page icons never appeared until that was fixed.

## Not tested

- **Minimize**, as above.
- **Print and developer tools:** both open system windows.
- **Real audio** playing, and the sound button appearing for it.
- **Escape closing a popup**, because test key events go to the page rather than to the browser's key handling. The click-away, second-press and menu-opening paths were tested.
- **Dragging tabs** between a private and ordinary window (it's refused in code only).
- **The native file picker** for adding extensions. A stand-in was used.
- **Windows and Linux:** nothing here has run on either.
- **Location prompts** and the real camera, as before.
- **The macOS title bar:** its buttons were never seen.

### A stray tab

Once, while testing with four addresses, a fifth "Google" tab appeared beside the first, was selected, and later wasn't there. It didn't come back in two more runs, so it isn't understood. A page can only open a tab during a click or key press, so it may have been a page doing that. If a stray tab appears, note which pages were open.
