# Milestone 6: workspaces, vertical tabs, split view, side panel, command bar, web clipper, screenshots, colour picker and the new icon

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
| Screenshot and annotate | **Done** and tested on macOS |
| Colour picker and font identifier | **Done** and tested on macOS |
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

## Screenshots

Menu or command bar: **Screenshot: what's showing** or **Screenshot: the whole page**. The picture opens in a new tab to mark up.

- **Capture:** taken through the page's DevTools protocol, so a whole page is one picture however far it scrolls (up to 16,384 px tall). A test page of 5 sections came out 1265 × 3200, with each section's colour where it should be. Only web pages can be captured.
- **Tools:** pen, arrow, rectangle, highlight, text, **hide with blocks** (turns a region into blocks, for covering a name or number), and **crop**. Six colours, three thicknesses, undo and redo (also Cmd/Ctrl+Z, and Shift for redo).
- **Save PNG:** goes to Downloads through the browser's usual download path, named like `Screenshot 2026-11-17 at 18.25.01.png`, and is the marked-up picture cut to the crop. Checked: the saved file exists.
- **Copy:** puts the marked-up picture on the system clipboard so it pastes as a picture. **macOS only for now.** On other systems it says so and points to Save. Checked on macOS: the clipboard held a PNG of the right size.
- **Private:** nothing is uploaded. The picture is held in memory for the new tab, handed over once, and only reaches disk if you save it.
- **Not restored:** a screenshot tab isn't brought back after a restart (it would be empty).

### How it works

- Two DevTools calls: `Page.getLayoutMetrics` (for the whole page's size) and `Page.captureScreenshot` (with `captureBeyondViewport` and a clip). The answers come back through a DevTools message observer, held only while a picture is being taken. Pictures over about 45 MB are refused, so a huge page can't stall the browser.
- The editor is `ui/screenshot.html`, an internal page like Settings. It draws on a canvas: the picture, then each mark in order, so undo just removes the last mark. Crop and the block effect are applied when saving, from the untouched picture.
- The browser hands the picture to the tab that was opened for it, once, when the page asks (`getScreenshot`). Copy sends the marked-up PNG back to the browser, which checks it's a PNG before putting it on the clipboard.
- Two things that didn't work and were changed: copying an image out of a page only puts its HTML on the clipboard (so Copy is done natively), and `Page.captureScreenshot` needs the layout metrics first to capture below the fold.

### Tested

On macOS, with real mouse events on the editor: visible-area and whole-page capture, every tool, undo and redo, text typed in, crop (export 950 × 570), Save (file in Downloads) and Copy (PNG on the clipboard). A screenshot of the editor was looked at. A unit test covers the screenshot tab not being restored.

### Not tested or not built

- **A region you drag out on the page.** Capture is the visible area or the whole page; crop in the editor does the rest.
- **Copy on Windows and Linux,** and the rest of this on both.
- **Move, resize or edit marks after drawing them.** Undo and redraw.
- **Capturing a page that's zoomed, in a split view,** or a very wide or tall page beyond the limit (it's cut at 16,384 px).
- **A keyboard shortcut.** It's in the menu and command bar.
- **Screen and window capture, and recording.** That's a separate piece (capture and a picker, in Phase 0) and still needs research.
- **The block effect** was drawn but, on a plain-coloured test page, can't be seen changing anything. Not checked on real text.

## Colour picker and font identifier

Two tools, from the menu, the command bar and the side panel's new **Colours** tab.

**Pick a colour from the page.** A picture of what's showing opens in the screenshot editor with the eyedropper ready. A magnifier follows the pointer, showing the pixels under it and the colour's `#RRGGBB` and `rgb()`. A click copies the hex code, shows it in a strip of the last few picked, and keeps it in the Colours tab (newest first, up to 60, a colour only once). The eyedropper is also one of the editor's tools, so it works on any screenshot.

**Identify fonts and colours of text.** The page shows a small card beside the pointer for whatever text it's over, and outlines that element: the font's name, size, weight (and italic), line height and letter spacing, the text colour, the colour behind it, and the full font list the page asked for. A click copies a one-line summary (`Georgia, 44px, weight 700, #112233 on #00AA00 (Georgia, serif)`) and stops; Escape or running it again stops too. It never changes the page, and links don't open while it's on.

**Colours tab:** each colour is a swatch you click to copy; hover for a remove button; "Copy all" puts the list on the clipboard one per line; and buttons start the two tools.

### Accuracy, and a decision for the owner

- Picking from a picture of the page is only as good as the picture. On this Mac, Chromium drew and captured in the screen's own wide-gamut colour space, so `#FF8800` read back as `#F78801` and `#00AA00` as `#32AA00`. That's no use for a colour picker.
- **Stardust now starts Chromium with `--force-color-profile=srgb`**, which makes it draw in sRGB, the space page colours are written in. With it, all three test colours came back exactly (`#FF8800`, `#00AA00`, `#112233`). The cost: it applies to everything, so wide-gamut (Display P3) colours in pages and video are shown as sRGB. For most sites nothing changes, but it's a change to how everything is drawn. It's one switch in `crates/cef-host/src/app.rs` and is skipped if the switch is already given on the command line. Decide whether to keep it.
- The font card shows the font the page *asked for* (the first in its list), not the one actually drawn when the first isn't installed. Finding the drawn one needs the DevTools `CSS.getPlatformFontsForNode`, which isn't used yet.

### How it works

- The picker reuses the screenshot capture, then reads pixels from the untouched picture (no marks), so a colour is the page's own.
- The font card is a script (`INSPECT_SCRIPT`) the browser runs in the active page's main frame. It's fixed text, takes no input, and draws inside a closed shadow root so the page's styles don't affect it.
- Saved colours are checked wherever they're read or added: only `#rgb` or `#rrggbb`, turned into upper-case `#RRGGBB`, with unit tests.

### Tested

On macOS with real mouse events: the magnifier and its label, picking three colours and checking them against the page's CSS values, the Colours tab (screenshot), Copy all, clicking a swatch (clipboard checked), and the font card (screenshot of the card, then a click and the clipboard checked, and the inspector stopped). Unit tests cover colour tidying, duplicates, the limit and bad input.

### Not tested or not built

- **Windows and Linux.** The colour accuracy fix and the picker are untested there.
- **Picking colours outside the page,** such as from another app or the screen (that would need screen capture).
- **The font actually drawn** (above), and a colour format choice (HSL, `rgb()` copy, Tailwind or CSS variable names).
- **Palettes from an image or a page,** contrast checking, and sharing colours with a design tool.
- The font card in frames inside a page, and on pages that stop mouse events from reaching it.
- **A keyboard shortcut.**

## Tests

99 tests in `bridge` pass (13 new: workspace list rules, ids, colours, saved-data checks). Clippy and `cargo fmt` clean. In the running browser: cookie separation, persistence across a hard kill and restart, tab restoration, opening a workspace with saved tabs, duplicate-name and delete-while-open refusals, rename, create and delete, and the menu and Settings card by screenshot.

## Not tested

- Several windows of one workspace at once, and tearing a tab out of a workspace window.
- Deleting a workspace and restarting to see its folder go.
- The icon in the Dock and Finder.
- Linux and Windows: nothing here has run on either.
- With many workspaces open, how much memory each profile adds.
