# Milestone 3 progress

Goal: with 20 tabs open, Stardust uses less memory than Chrome, and the everyday gaps left by Milestone 2 are closed.

This milestone was built and tested on macOS (Apple Silicon), the first time Stardust has run there. The Linux work in earlier milestones hasn't been re-run with these changes.

## Status

| Piece | Status |
|---|---|
| Builds and runs on macOS | **Done**, with the sandbox on. Only checked from processes and logs, because no screenshot could be taken in the test session |
| Tab discarding | **Done**: see below |
| 20-tab test against Chrome | **Done** in `perf`, which now also runs on macOS. **The goal isn't met yet:** see below |
| Closing a tab returns to the tab that opened it | **Done** |
| Find-in-page match case | **Done**: an Aa button in the find bar |
| Find bar closes when the page changes | **Done**. A change of only the part after `#` keeps it open |
| Moving tabs with the keyboard | **Done**: Ctrl+Shift+Page Up and Page Down, or Command on macOS |
| Opening addresses from the command line | **Done**: `stardust a.com b.com` opens each as a tab, all loading at once, as Chrome does |
| Setting to turn discarding off | **Done**, in Settings |
| The menu shares the toolbar's process | **Done**: one less renderer per window, see below |

These were listed in the draft roadmap, but turned out to exist already from the end of Milestone 2: address bar suggestions, clearing cookies and cache, Back and Forward surviving a restart, a scrolling tab strip, bookmark folders and the bookmarks bar, saved download history, asking where to save downloads, and a warning when quitting during a download.

## Tab discarding

Once a minute, the browser looks at every loaded tab in every window. A background tab unloads when it hasn't been shown for 30 minutes, or when more than 10 tabs are loaded, starting with the one shown least recently.

An unloaded tab keeps its place, title and Back and Forward list, and loads again when selected, the same way restored tabs already did.

These tabs are never unloaded:

- The tab showing in each window.
- Tabs playing sound.
- Tabs still loading.
- A tab with a page dialog waiting.
- Any tab in a window with an active download, because a download stops with the tab that started it, and which tab that was isn't tracked yet.
- Blank tabs, which cost almost nothing.

The policy is in `crates/bridge/src/discard.rs`, with unit tests.

### Not handled yet

- **Unsaved form text:** a tab with text typed into a form can be unloaded, and the text is lost. Chrome avoids this, and it needs a check from the page's renderer process.
- **Memory pressure:** discarding goes by time and tab count, not by how much memory the computer has left.
- **No sign in the tab strip:** an unloaded tab looks the same as a loaded one.

### How a tab unloads

The tab's browser closes normally, not forced: a forced close never finishes for a browser shown in a window's view, so its renderer process stayed and no memory was saved. A normal close runs the page's unload handlers. If the page asks "Leave site?", which usually means unsaved work, the prompt isn't shown and the tab stays loaded for good.

## Performance

The 20-tab figures are from the final run of `cargo run --release -p perf -- dist --chrome "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"` on an Apple Silicon Mac, on 2026-11-06. Stardust was a debug build, because the macOS bundler only makes those. Memory is each process's footprint, as Activity Monitor shows it, so these figures can't be compared with the Linux ones in earlier milestones.

| Measurement | Result | Limit |
|---|---|---|
| Installed size | 385 MB | 450 MB |
| Startup to first request | 0.5 to 8.8 s | 15 s |
| RAM with one simple tab | 259 to 459 MB | 500 MB |
| Idle CPU | 0 to 10% | 5% |
| RAM with 20 tabs | **767 MB** | Chrome's figure: **722 MB** |

**Stardust used about 6% more memory than Chrome with 20 tabs,** so the milestone's goal isn't met. Before the menu shared the toolbar's process, it was 790 MB against 711 MB, about 11% more. Runs before that were affected by the test problems listed below.

The one-tab and idle CPU figures varied a lot between runs, while other work was running on the same machine, so treat them as rough. Idle CPU was 1% in the clean run.

### Where the memory goes

`perf` now prints memory by kind of process. From the run before the menu change:

| Process | Stardust | Chrome |
|---|---|---|
| Browser | 110 MB | 152 MB |
| GPU | 47 MB | 36 MB |
| Renderers | 575 MB in 13 processes, about 49 MB each | 493 MB in 22 processes, about 22 MB each |
| Network and storage | 48 MB | 20 MB |

Stardust had unloaded 10 of its 20 tabs and still used more, because each of its renderer processes uses more than twice as much as Chrome's.

The largest single cause found: in every Stardust renderer, about 12.6 MB of the CEF framework's constant data (`__DATA_CONST`) is private to that process. In Chrome's renderers the same data is shared, and costs almost nothing per process. With 13 renderers, that's about 160 MB, more than the whole gap to Chrome.

This comes from CEF, not Stardust's own code. Checked and ruled out:

- **Code signing:** the same with ad-hoc signing and with a real Apple Development certificate.
- **The sandbox:** the same with it turned off.
- **How the framework was built:** both frameworks target macOS 13, with the same SDK and the same fixup format.

Chrome starts its renderers from "Aperitif" helper apps, with a `libaperitif` library that CEF builds don't include. That's the likely difference, but it hasn't been confirmed.

### Options

1. **Ask CEF upstream** whether its framework's data can be shared between processes, as Chrome's is. This would help every CEF browser on macOS.
2. **Use fewer renderer processes of Stardust's own.** Done for the menu: see below.
3. **Unload more tabs,** by lowering the limit of 10 loaded tabs. This would meet the goal on paper, but more tabs would reload when selected.

### The menu shares the toolbar's process

The menu under the toolbar, for address suggestions and bookmark folders, used to be its own browser with its own renderer process, about 28 MB in every window, created at startup.

Now the toolbar page opens the menu itself with `window.open`, and writes the menu's page into it. Chromium keeps a window opened that way in its opener's process, so the menu costs only its page, not a process. The browser still places it as an overlay under the toolbar and fills it in as before. The menu sends clicks through the toolbar's bridge.

A page can only open a window during a key press or click, so the menu opens on the first one in the toolbar, which is also the first moment it can be needed. The new window takes focus for a moment, so the toolbar ignores that loss of focus, then puts focus back where it was.

The toolbar may open no other window: any other popup it tries is refused. Before this, nothing stopped it.

Tested by sending real key presses and clicks to the toolbar through the debugging protocol, with one tab open:

- Renderer processes: 3, down from 4.
- Typing straight away, or clicking the address bar and then typing: no letters were lost, the address bar kept focus, and the menu showed the matching suggestion.
- Clicking the suggestion opened it and closed the menu.
- Not tested: bookmark folder menus, which open the same way, and anything on screen, which wasn't looked at.

In the 20-tab test the menu never opens, because nothing is typed, so the test shows the whole 28 MB saving. In real use the menu opens early, and the saving is the process, less the menu page's own memory.

Results with this change:

| Process | Stardust | Chrome |
|---|---|---|
| Browser | 111 MB | 156 MB |
| GPU | 47 MB | 39 MB |
| Renderers | 556 MB in 12 processes | 485 MB in 22 processes |
| Network and storage | 48 MB | 27 MB |
| **Total** | **767 MB** | **722 MB** |

The remaining 12 renderers are 10 loaded tabs, the toolbar and one spare that Chromium keeps ready for the next page.

### How the 20-tab test works

- Each tab is its own site, `tabN.localhost`, served by `perf`. On a single site, both browsers share renderer processes between tabs, which isn't how real browsing looks.
- Stardust gets all 20 addresses on its command line and loads them all at once.
- Chrome delays background tabs at startup, so its tabs open one at a time through its debugging port, which loads each one.
- The test waits until every page has been requested, then 75 seconds more, so Stardust's first unloading check has run.

Problems found while building it, now fixed:

- The test server handled one connection at a time, and browsers open connections early and leave them idle, so pages never finished loading. It now gives each connection its own thread.
- Passing `--user-data-dir` to Stardust stopped it loading pages, because it conflicts with the profile path Stardust sets itself. Only Chrome gets it now.
- On macOS, the test's throwaway profile has no keychain, so both browsers showed a "Keychain Not Found" dialog that held up loading. They now get `--use-mock-keychain`.
- Chrome kept its debugging connection open, so the test waited forever for it to close. It now reads only the reply's stated length.

Running a second copy of a rebuilt `dist/stardust.app` while an older copy is still open makes macOS kill the new one at launch, with exit code 137.

## Tests

110 unit tests pass, up from 75 at the end of Milestone 2. The new ones cover the discard policy, returning to the opener tab, moving tabs, the new shortcuts and the `perf` parsers.

Not tested:

- **What's on screen:** the find bar's Aa button, the Settings switch and unloaded tabs reloading when clicked were never seen, because no screenshot could be taken. Their logic is covered by unit tests and logs only.
- **Keyboard and mouse:** nothing was clicked or typed. Opener return and keyboard tab moves have unit tests only.
- **Tabs playing sound:** that they stay loaded hasn't been tried.
- **Linux and Windows:** not re-run with these changes.
