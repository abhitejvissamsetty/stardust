# Milestone 6: workspaces, vertical tabs, split view, side panel, command bar, web clipper and the new icon

Built and tested on macOS (Apple Silicon) on 2026-11-17. This is the first piece of Phase 1 of [creator-roadmap.md](creator-roadmap.md). Tested by driving the browser through Chromium's debugging protocol with real clicks, and by screenshots of Stardust's own pages. The native window frame wasn't looked at.

## Status

| Piece | Status |
|---|---|
| New app icon (macOS) | **Done.** Built into the app bundle by `cargo xtask bundle`. Not looked at in the Dock or Finder |
| Workspaces: own cookies, logins and site storage | **Done** and tested, including after a hard kill and restart |
| Workspaces: own open tabs, restored on switching | **Done** and tested |
| Saved logins kept apart per workspace | **Done** and tested |
| Workspace button and switcher in the toolbar | **Done**, checked by screenshot |
| Settings → Workspaces (create, rename, colour, delete) | **Done**, tested |
| Vertical tabs: a tab list down the left, collapsible to icons | **Done** and tested |
| Split view: two pages side by side | **Done** and tested, but not by clicking a pane |
| Side panel: notes, to-do list and snippets | **Done** and tested |
| Command bar: find tabs, workspaces and commands | **Done** and tested |
| Web clipper: save pages and selected text to the side panel | **Done** and tested |
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
| Local storage, IndexedDB, caches | Site permission answers, downloads, settings |
| Saved logins, and "never save" choices | Extensions, which **only run in Personal** |
| Open tabs and their back/forward lists | |

### Saved logins

Logins are kept per workspace, so two accounts on one site don't mix. A login saved in Channel A is stored under the key `w1|<site>` (in Stardust's database and in the system password store); Personal keeps the plain site as before, so nothing already saved changed. The key button only appears for, and only fills, the current workspace's login. Settings → Passwords lists every login with its workspace's name in brackets, and Remove and Allow saving work on the right one. Tested: Personal saved `ann` and Channel A saved `bob` for the same site; Channel A showed no key until it had its own login, and each window filled only its own account.

### How it works, and two things learned

- A workspace's profile is a CEF request context with its own `cache_path`.
- **CEF only accepts a profile folder that is a direct child of the data folder.** A folder one level deeper silently becomes a memory-only profile: cookies were separate but vanished on restart. Found by testing a restart. The folders are now `Workspace-w1` and so on, and only folders named exactly like that are ever deleted.
- **A page made before the profile has started fails.** A workspace window waits for the profile's "initialized" signal before making its toolbar and tabs, with a 4-second fallback. Before this, opening a workspace crashed the browser every time.
- Workspace ids are `w` and a number, never reused, so a deleted workspace's data can't return under a new one. They become folder names, so nothing else is accepted.

## Vertical tabs

Settings → "Show tabs down the side", or "Show tabs down the side" in the menu, moves the tabs from a row across the top to a list down the left. The setting applies to every window at once, switches live, and is remembered across restarts.

- **The list:** a "New tab" button, a collapse button, the tab rows (page icon or spinner, title, sound and mute button, close button), and the current workspace's name once there is more than one. The selected tab has an accent bar. Unloaded tabs are dimmed. Middle click closes. Drag a row to reorder. Collapsed, it's 56 px wide and shows icons only; expanded, 264 px.
- **The toolbar** loses its row of tabs and gets 40 px shorter.
- **A private window** shows "Private" at the top of the list and keeps its dark theme.
- **Tab commands** (select, close, move, new tab, mute) all go through the same code as the tab row.

### How it works

- Each window's pages now sit in a box of their own under the toolbar, and the tab list is a second small browser view in that box, loading its own page (`ui/sidebar.html`). It costs one more renderer process while vertical tabs are on, and none when they're off.
- The browser trusts what its pages send, so the tab list gets **its own, smaller set of allowed commands** (`UiCommand::allowed_from_sidebar`, with a test), and can only ever show its own page. It opens no windows.
- The list is built and removed live. Turning it off closes its browser properly, which needed `do_close` to let go of the view first, as the extension popup does. A layout change that runs inside a toolbar's own message is deferred, or that toolbar wouldn't take its new height.

### Tested

Page width equals the window minus the list (1016 + 264 = 1280, and 1224 with it collapsed). With real mouse events: selecting a row, New tab, closing a tab, collapsing. Moving a tab by command. Turning it on, off and on again from each window, and across a restart. Closing a window that has a list, and closing the last window, which quits cleanly. The list was checked by screenshot.

### Not tested or not built

- **Dragging a row** with a real mouse: the reorder code is written, but only the move command was driven.
- **Groups, search and pinned tabs** in the list, which the roadmap also lists. Not built.
- **A window that's hidden behind another** keeps a stale toolbar height until it's shown, because Chromium doesn't resize hidden pages. Seen in tests, and right once shown.
- Resizing the list by dragging its edge. The width is one of two sizes.
- Keyboard use of the list (it has roles, but Tab/arrow navigation isn't designed).
- Windows and Linux.

## Split view

Menu → "Split view: two pages side by side" opens a new tab beside the current one, so two pages show at once, each half the width, with a thin line between. The same menu item reads "Close split view" and goes back to one page.

- **The active pane** is the one the toolbar is about: its address, back and forward, find, zoom and reload. Clicking the other pane makes it active. Picking it in the tab strip or the vertical list does the same.
- **Picking another tab** from the strip or list replaces the pane that was in use, and leaves the other pane alone.
- **Marks:** the two tabs in the split are underlined in the accent colour in the tab row, and have a small square in the vertical list.
- **Closing a pane's tab** ends the split and keeps the other page.
- **Unloading:** the two tabs are never unloaded to save memory.
- **Layout:** the pages box holds both views and the left one is always put first. It works with vertical tabs (each pane 507 px of a 1280 px window with the list open), and in private windows and workspaces.

### Tested

Opening, navigating the right pane, switching the active pane from the strip, replacing a pane with a third tab, closing a pane, closing the split, and the same with vertical tabs on. Page widths were measured (639 + 2 + 639 = 1280) and the real window was looked at in a screenshot of the screen, which showed the two pages in the right order.

### Not tested or not built

- **Clicking a pane to make it active.** The code uses CEF's focus callback, but a real click couldn't be sent from here (no accessibility permission for scripted clicks), and test clicks through the debugging protocol don't move native focus.
- **Dragging the divider** to resize the panes: always half and half.
- **Splitting with an existing tab:** the split always opens a new tab. Moving an open tab into a pane isn't built.
- **A top and bottom split**, and more than two panes.
- **The split isn't saved:** after a restart, the two tabs come back as ordinary tabs.
- **A shortcut key.** It's in the menu only.
- Windows and Linux.

## Side panel

A panel at the right of the pages, opened from the toolbar's panel button, the menu, or Cmd/Ctrl+Shift+E. It's per window, 320 px wide, and works with vertical tabs and split view (264 + 345 + 345 + 320 + gaps = 1280 in a test with all three on).

- **Notes:** one page of text, saved as you type a moment after you stop (up to 100,000 characters).
- **To-do:** add, tick, remove, and "Clear done". The newest is at the top. Up to 200, 300 characters each.
- **Snippets:** text you reuse, such as a sign-off, with an optional title. **Copy** puts it on the clipboard. "Save this page's link" keeps the active page's title and address as a snippet. Up to 100, 5,000 characters each.
- **Shared:** everything is shared by every window and workspace, and a change in one window's panel shows in the others at once. It's saved with the browser's other settings, so it comes back after a restart.
- **Private windows** can open the panel too, but it shows and edits the same shared notes, so nothing written there is private. See the limits.

### How it works

- The panel is a third small browser view of the browser's own (`ui/sidepanel.html`), docked at the right end of the pages box and always kept last. It loads only its own page, opens no windows, and, like the tab list, is held to its own set of commands (`allowed_from_side_panel`, with a test). The tab list and the panel now share one client, told apart by which of the two it is for.
- Data is validated whenever it's read or changed: limits on counts and lengths, control characters turned to spaces, and ids that are never reused (`crates/bridge/src/sidepanel.rs`, with tests).
- The page is a `data:` page, which can't use the clipboard's own API, so Copy uses a hidden field and the old copy command.

### Tested

With real mouse and keyboard events: typing notes, adding and ticking to-dos, saving a snippet, and Copy (the clipboard held the snippet). Sharing between two windows, saving across a restart, closing the panel from its own button, and the page returning to full width. Page widths in the three-way layout. The page was checked by screenshot.

### Not tested or not built

- **A shortcut checked on the keyboard:** it's unit-tested only.
- **A panel kept open between runs:** it always starts closed.
- **Resizing the panel** by dragging.
- **Notes or to-dos per workspace.** They're global, so a workspace for a client shows the same notes. Splitting them is a decision for the owner.
- **Private windows** share the same data (above).
- **Inserting a snippet into a page,** or saving a selection from a page. Only Copy and the page's link are there.
- **Search or tags** in the panel, and export. The data is plain JSON in the settings table.
- Windows and Linux.

## Command bar

Cmd/Ctrl+Shift+A (Chrome's "Search tabs" shortcut), the menu item, or typing `>` at the start of the address bar. The address bar becomes a quick switcher and shows up to 8 results under it.

- **What it finds:** the other open tabs (Enter switches to one), the other workspaces (Enter opens one), and about 25 commands: new tab, window and private window; reopen and close tab; split view; side panel; vertical tabs; reload, find, bookmark, zoom, print, developer tools, full screen; History, Downloads, Bookmarks, Extensions, Settings; quit. Labels follow the current state ("Close split view" while one is open) and show each command's shortcut.
- **Ranking:** a title that starts with what you typed comes first, then one with a word that starts with it, then one that contains it, then one with the letters in order. Ties keep tabs, then workspaces, then commands. An empty search lists the tabs, then the commands.
- **Keys:** the first result is ready, so Enter runs it; Up and Down move between results; Escape leaves the bar and restores the address. Clicking a result also runs it.
- **Private:** it never sends what you type to a search engine or a website, and it's all built in the toolbar's page from what's open, so nothing needs a round trip. Workspaces aren't listed in private windows.
- Built into the address bar's existing suggestion list rather than as a new window, so it adds no process and no new commands for a page to send.

### Tested

Real key presses and a real mouse click in the running browser: opening it, ranking ("split" gives Split view first, "logi" gives the Login tab, "chan" gives a workspace), Down and Up, Enter on a tab, on a command (it started a split view) and on a workspace result, Escape, and clicking a result.

### Not tested or not built

- **The shortcut on the keyboard,** only its unit test. The menu item and `>` work.
- **Bookmarks and history** aren't searched here: typing normally in the address bar still does that.
- **Recent use:** results aren't ordered by what you used lately, and it doesn't learn.
- **Your own commands**, and arguments such as "move tab to workspace…".
- Windows and Linux.

## Web clipper

"Clip this page" saves the page you're on, with the text you've selected on it, to a **Clips** tab in the side panel. Cmd/Ctrl+Shift+S, the menu, the command bar, or the button in the Clips tab. A "Clipped" message shows beside the address bar.

- **What a clip keeps:** the page's title and address (from the browser, not from the page), a note, and the date. The note is the text selected on the page, up to 2,000 characters, or else the page's own description (its `description` or `og:description` tag).
- **One clip per page:** clipping the same address again replaces the earlier clip and moves it to the top. Up to 300 clips.
- **In the panel:** each clip shows its title (click to open it in the current tab), the site and date, and the note. Copy link and remove sit beside it. **Copy all as Markdown** puts every clip on the clipboard as `## [title](address)` with the note quoted under it, so clips paste into notes, a document or a repository. That's the "never a hostage" rule: they leave in a plain format.
- **Web pages only:** `http` and `https`. A blank tab, an internal page or a `data:` page isn't clipped, and the saved address is checked again when read.
- **Private windows:** clipping is refused ("Clips aren't kept in private windows"), since clips are kept.
- **Shared** with every window and workspace, like the rest of the side panel, and saved with the browser's settings.

### How it works

- The browser asks the page's own process for the selection and description with a message (`clip-request`), and it answers (`clip-result`). The reply is accepted only for a tab a clip was asked for, only once, and only from its main frame, so a page can't add clips by itself. A page that never answers is clipped without text after 1.5 seconds.
- The script that reads the page (`CLIP_SCRIPT`) returns plain text, cut at fixed lengths. What comes back is untrusted: it's tidied (control characters turned to spaces) and only ever set as text.
- The panel's own commands for clips (`clipPage`, `removeClip`, `openClip`) are in its allowed list. `openClip` takes only an id and looks the address up in what's saved, so the panel can't make the browser open an address of its choosing.

### Tested

In the running browser: clipping with a selection (the note was the selected sentence), clipping again with nothing selected (the page's description replaced the note, and there was still one clip), a second page, a blank tab (refused with a message), a private window (refused), the Clips tab by screenshot, Copy all as Markdown (checked on the clipboard), opening a clip by clicking its title, and removing one. Unit tests cover once-per-page, newest-first, web-only addresses, tidying and limits, and checking saved data.

### Not tested or not built

- **The shortcut on the keyboard,** only its unit test. The menu, the command bar and the panel button work.
- **Folders, tags and search** for clips, and an export to Notion or Obsidian. Only Markdown is there.
- **Clipping the page's text or a screenshot,** only a selection or the description. No offline copy.
- **Clips from a frame inside a page,** such as an embedded document: only the main frame is read.
- **Clips per workspace.** They're shared.
- **Seen once and not understood:** during testing the side panel closed by itself and a "DevTools" page briefly appeared in the debugging list. It didn't come back in the steps that followed. If it does, note what was open.
- Windows and Linux.

## Tests

99 tests in `bridge` pass (13 new: workspace list rules, ids, colours, saved-data checks). Clippy and `cargo fmt` clean. In the running browser: cookie separation, persistence across a hard kill and restart, tab restoration, opening a workspace with saved tabs, duplicate-name and delete-while-open refusals, rename, create and delete, and the menu and Settings card by screenshot.

## Not tested

- Several windows of one workspace at once, and tearing a tab out of a workspace window.
- Deleting a workspace and restarting to see its folder go.
- The icon in the Dock and Finder.
- Linux and Windows: nothing here has run on either.
- With many workspaces open, how much memory each profile adds.
