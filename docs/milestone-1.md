# Milestone 1 results

Tested on 2026-11-01 on Arch Linux. Tests used real keyboard and mouse input on a private virtual display, with screenshots checked by eye.

## What works on Linux

| Feature | Result |
|---|---|
| Toolbar with Back, Forward, Reload and Stop | Works |
| Address bar: URLs, host names, local addresses and search | Works |
| Back and Forward, by keyboard and button | Keyboard and Back button tested; Forward button shares the same command path |
| Loading bar and Reload changing to Stop | Works |
| Stopping a slow load | Works, and the address returns to the page on screen |
| Error page with the failed URL | Works, and Back and retry both behave correctly |
| Escape restoring the address | Works |
| Ctrl+L, Alt+Left, Alt+Right, F5 and Ctrl+W | Tested |
| Pasted `javascript:` becoming a search | Works |
| Web pages can't reach the toolbar bridge | `window.browserBridge` is undefined on a normal page |
| Sandbox | Renderer processes run with seccomp and their own PID namespaces |
| Wayland | The toolbar and page render in a nested sway session. Input wasn't tested there |
| Clean exit when the window closes | Exit code 0 |

13 unit tests cover address parsing, shortcuts, message parsing and error page escaping.

## Memory

| Page | Before the toolbar | With the toolbar |
|---|---|---|
| example.com | 320 MB | 323 MB |
| YouTube home | 610 MB | 643 MB |

Each figure is a single run, and YouTube varies a lot between runs. These were measured before the last small fixes, which shouldn't change them noticeably.

## Browsing test

Run with real keyboard and mouse input on a private display, without signing in anywhere.

**Sites loaded and rendered correctly, with no crashes:** Wikipedia, GitHub, Stack Overflow, BBC News, Reddit, Amazon India, X, Gmail's landing page, MDN, DuckDuckGo, YouTube and Google.

| Behavior | Result |
|---|---|
| Mouse wheel and Page Down scrolling | Works |
| Typing into a page's own search box | Works |
| YouTube VP9 video | Plays: the picture kept changing between screenshots |
| Links that open a new window | Open in the current page |
| Clicked `window.open` | Opens in the current page |
| `window.open` without a click | Blocked |
| `alert` and `confirm` dialogs | Work |
| Dropdowns, text input and copy/paste | Work |
| File picker on the real desktop | Opens and cancels correctly. Choosing a file wasn't tested |
| Location request | Denied immediately. Before the fix, the page waited forever |
| Alt+D, F6, Ctrl+Shift+R, Ctrl+Q and Escape to stop | Work |
| Toolbar Forward button | Works |

**Test note:** on a virtual display, the file dialog opens on the real desktop through the system portal and blocks the browser window until it closes. This is a test setup effect, not a bug seen on the real desktop.

## Not done or not verified

- **Downloads:** clicking a download does nothing and shows no message. Planned for Milestone 2.
- **Signed-in use:** logging in, and sign-in popups that rely on `window.opener`, weren't tested and may fail until tabs arrive in Milestone 2.
- **Real-site links:** links were clicked on a local test page, not on the live sites.
- **Long use:** the test ran for minutes, not hours, and no person has judged how it feels to use.
- **Windows and macOS:** type-checked only, never run. The Milestone 0 blocker still applies.
- **macOS shortcuts:** untested.
- **H.264 and AAC:** still don't play, so some sites' videos will fail.

## Known limits of Alloy style

The window uses CEF's Alloy style, because it draws no Chrome UI of its own. As a result, these need their own work:

- **Downloads:** no handler yet, so nothing happens when a download starts. Planned for Milestone 2.
- **Permission prompts:** camera, microphone and location requests are denied automatically until there is a prompt UI.
- **Password and form autofill:** not available.
- **Extensions:** the Milestone 4 extension review needs to account for this choice.

## Decisions made during this milestone

- **Default search:** DuckDuckGo, as a privacy-friendly default. It's the `SEARCH_URL` constant in `crates/bridge/src/address.rs`, and settings arrive in Milestone 2.
- **Plain JavaScript:** the toolbar uses JavaScript instead of the planned TypeScript, to avoid adding a build tool for one small file. It's worth revisiting as the UI grows.
