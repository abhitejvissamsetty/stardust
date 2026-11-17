# Milestone 6: workspaces and the new icon

Built and tested on macOS (Apple Silicon) on 2026-11-17. This is the first piece of Phase 1 of [creator-roadmap.md](creator-roadmap.md). Tested by driving the browser through Chromium's debugging protocol with real clicks, and by screenshots of Stardust's own pages. The native window frame wasn't looked at.

## Status

| Piece | Status |
|---|---|
| New app icon (macOS) | **Done.** Built into the app bundle by `cargo xtask bundle`. Not looked at in the Dock or Finder |
| Workspaces: own cookies, logins and site storage | **Done** and tested, including after a hard kill and restart |
| Workspaces: own open tabs, restored on switching | **Done** and tested |
| Workspace button and switcher in the toolbar | **Done**, checked by screenshot |
| Settings → Workspaces (create, rename, colour, delete) | **Done**, tested |
| Icon for Windows and Linux | **Not done**, see below |

## The icon

`assets/icon/stardust.png` (1254 px, the source) and `stardust.icns` (made with `sips` and `iconutil`). `cargo xtask bundle` copies the `.icns` into the app's `Resources` and sets `CFBundleIconFile`. Windows needs an `.ico` and Linux a set of PNGs, and neither has been made or wired into their bundlers.

## Workspaces

A workspace is a named, coloured set of tabs with a profile of its own, on disk. One per channel or brand keeps each account signed in separately. The original profile is the **Personal** workspace: it has no entry in the list and can't be removed or renamed.

- **Own profile:** a workspace's cookies, logins and site storage live in `Workspace-<id>` inside Stardust's data folder. Cookies set in one workspace aren't visible in another, and survive a restart. Tested with the same site and the same cookie name in two windows.
- **Own tabs:** a window belongs to one workspace. Its tabs are saved under that workspace, so opening it later brings back where it was left, even after its window was closed. A workspace that is open in a window is shown, not opened twice.
- **Switching:** the toolbar's coloured dot and name open a menu of workspaces ("This window" and "Open" mark where each is). Choosing one shows its window or opens it. "New window" from a workspace window opens in the same workspace.
- **Managing:** Settings → Workspaces creates, renames, recolours and deletes. Names are 1 to 40 characters and unique; at most 20 workspaces. Colours come from a fixed list, never free text. A workspace can't be deleted while a window of it is open. Deleting erases its data the next time Stardust starts.
- **Tabs can't move** between windows of different workspaces, as with private windows.
- **Private windows** belong to no workspace, and hide the button.
- **Window titles** end in the workspace's name outside Personal.

### What is and isn't separate

| Separate per workspace | Shared by all |
|---|---|
| Cookies, and so who is signed in | History and bookmarks |
| Local storage, IndexedDB, caches | Saved passwords (the key button fills the most recently used login for the site, from any workspace) |
| Open tabs and their back/forward lists | Site permission answers, downloads, settings |
| | Extensions, which **only run in Personal** |

The shared saved passwords are a real limit for the main use case: two accounts on the same site share one list, so the key button can offer the wrong account's login. See [remaining-work.md](remaining-work.md).

### How it works, and two things learned

- A workspace's profile is a CEF request context with its own `cache_path`.
- **CEF only accepts a profile folder that is a direct child of the data folder.** A folder one level deeper silently becomes a memory-only profile: cookies were separate but vanished on restart. Found by testing a restart. The folders are now `Workspace-w1` and so on, and only folders named exactly like that are ever deleted.
- **A page made before the profile has started fails.** A workspace window waits for the profile's "initialized" signal before making its toolbar and tabs, with a 4-second fallback. Before this, opening a workspace crashed the browser every time.
- Workspace ids are `w` and a number, never reused, so a deleted workspace's data can't return under a new one. They become folder names, so nothing else is accepted.

## Tests

99 tests in `bridge` pass (13 new: workspace list rules, ids, colours, saved-data checks). Clippy and `cargo fmt` clean. In the running browser: cookie separation, persistence across a hard kill and restart, tab restoration, opening a workspace with saved tabs, duplicate-name and delete-while-open refusals, rename, create and delete, and the menu and Settings card by screenshot.

## Not tested

- Several windows of one workspace at once, and tearing a tab out of a workspace window.
- Deleting a workspace and restarting to see its folder go.
- The icon in the Dock and Finder.
- Linux and Windows: nothing here has run on either.
- With many workspaces open, how much memory each profile adds.
