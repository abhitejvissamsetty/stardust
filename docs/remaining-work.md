# Remaining work

What's left after Milestone 4, written as a brief for whoever picks it up next, person or agent. Each item says why it matters, what's already known, and what "done" looks like. Written on 2026-11-06.

Read first: [roadmap.md](roadmap.md), then [milestone-3.md](milestone-3.md) and [milestone-4.md](milestone-4.md) for detail and measurements.

## Context

- **Stardust** is a Rust browser on Chromium through CEF 156 (`cef-rs`), using CEF's Alloy style, so all browser UI is Stardust's own web pages (`ui/`).
- **Branch:** Milestones 3 and 4 are on the local branch `milestone-3` (commits `28f87ea` and `6c55f95`), not merged into `main` and not pushed. The project isn't on GitHub yet, so CI has never run.
- **The roadmap is a draft.** The original plan isn't in the repo. Milestones 3 and 4 were drafted from hints in the earlier docs. If the real plan turns up, check it against this list.
- **Platforms:** Linux ran Milestones 0 to 2. macOS (Apple Silicon) ran Milestones 3 and 4. Windows has never run.

## Decisions waiting on the owner

| Decision | Options | Blocks |
|---|---|---|
| Update server | Any HTTPS host serving a folder. GitHub Releases once the project is on GitHub | Updates reaching anyone |
| Real release signing key | Make with `cargo xtask keygen`, keep the private key offline | Updates reaching anyone |
| Build machine for custom CEF | A cloud VM or a bigger computer, see section 7 | H.264 and AAC video |
| Extensions | Stay without, or move to CEF's Chrome style, see below | Nothing now |

## 1. Milestone 2: a week of daily use

A contributor uses Stardust as their main browser for a week and writes down what breaks. Needs a person. Fix what's found, and record it in `milestone-2.md`.

## 2. Milestone 3: memory against Chrome

**Goal:** with 20 tabs, use less memory than Chrome. **Now:** 767 MB against Chrome's 722 MB on macOS (`cargo run --release -p perf -- dist --chrome "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"`).

What's known:

- Each Stardust renderer process uses about 49 MB, against Chrome's 22 MB for the same page.
- About 12.6 MB of that is the CEF framework's `__DATA_CONST`, private to every renderer, where Chrome's is shared. Ruled out as causes: code signing (ad-hoc and a real certificate), the sandbox, build settings. Chrome starts renderers from "Aperitif" helper apps with a `libaperitif` library that CEF doesn't have, which is the likely difference, unconfirmed. Check with `footprint -v -p <renderer pid> | grep __DATA_CONST`.
- The menu under the toolbar already shares the toolbar's process. Remaining renderers with 20 tabs: 10 loaded tabs, the toolbar and Chromium's spare.
- Stardust was a debug build, because cef-rs's macOS bundler only makes those (`xtask/src/main.rs`). Chrome is a release build.

Ideas, untried:

1. Ask CEF upstream whether framework data can be shared between renderers as Chrome does. Biggest win, outside this repo.
2. Make a release build on macOS and measure again.
3. Turn off Chromium's spare renderer (about 27 MB), at a small cost to navigation speed.
4. Lower the 10-loaded-tabs limit in `crates/bridge/src/discard.rs`. Meets the number, but more tabs reload when clicked.

**Done when:** `perf` with `--chrome` passes, on macOS and Linux.

## 3. Milestone 3: unfinished parts of tab discarding

In `crates/bridge/src/discard.rs` and `crates/cef-host/src/client.rs`:

- **Unsaved form text** in an unloaded tab is lost. Pages that ask "Leave site?" are kept, but most forms don't ask. Needs a check from the renderer process.
- **Memory pressure:** discarding goes by time and tab count, not by how much memory is free.
- **No sign in the tab strip** that a tab is unloaded.
- **Downloads:** any active download keeps every tab in its window loaded, because which tab started it isn't tracked.

## 4. Milestone 4: updates reaching people

The updater works end to end on macOS (see [milestone-4.md](milestone-4.md#updates)). Left:

1. Make the real signing key, replace `crates/update/release-key.pub`, and commit. The current key is a test key whose private half is in `~/.stardust-keys/test-release.key` on the Mac that made it.
2. Choose the server, and build releases with `STARDUST_UPDATE_URL=https://…/manifest.json`.
3. **macOS signing and notarization** for releases. Not set up. An update replaces the app with whatever is in the signed package.
4. **Install folders an administrator owns,** such as an app installed for all users, can't update. Needs a privileged helper or a per-user install.
5. Run the update flow on Windows. Linux passed on 2026-11-24.
6. Then remove the README's warning against logins and banking.

**Done when:** an installed build updates itself to a newer signed build on all three systems.

## 5. Windows

Never run. The code type-checks only. Needs a Windows computer or CI: build with `cargo xtask bundle --release`, which starts through CEF's `bootstrap.exe` with the sandbox on, then repeat the Milestone 1 to 4 tests. Expect platform bugs: `crates/cef-host/src/client.rs` has Windows branches (shortcuts, opening files) never exercised.

## 6. Linux re-run

Nothing from Milestones 3 and 4 has run on Linux. In particular:

- **VA-API:** Stardust now adds `AcceleratedVideoDecodeLinuxGL,VaapiVideoDecoder` when a `*_drv_video.so` driver is found (`crates/bridge/src/video.rs`). Check `chrome://gpu` shows hardware decoding, and measure CPU on a video as in Milestone 0.
- **Saved passwords** use the Secret Service through the `keyring` crate, which needs D-Bus (`libdbus-1-dev`, added to CI). Untested.
- **Cookie encryption** uses the desktop keyring when it's unlocked. Stardust checks it first (`crates/cef-host/src/secret_store.rs`): a locked keyring, or none, used to stop every page loading, because Chromium waited forever for an unlock prompt that couldn't appear. Now Stardust falls back to Chromium's built-in key, turns off saving passwords for that run, and says so. Tested on 2026-11-24 with unlocked, locked and missing keyrings. Cookies saved with the keyring stay readable once it's unlocked again. Not tested: KDE's KWallet, which Stardust leaves to Chromium.
- **`perf`** has Linux code paths (`/proc`, PSS) that changed in Milestone 3.

## 7. H.264 and AAC video

Stock CEF builds leave these out, so many sites' videos fail. **Decided on 2026-11-25: Firefox's way.** Stardust ships no H.264 or AAC decoders of its own and uses the operating system's, which the OS maker has licensed. Brave's way (shipping Chromium's FFmpeg decoders) was set aside over patent licensing: the last US H.264 patent in the Via LA pool is reported to expire on 29 November 2027, and AAC-LC's status isn't confirmed by any licensor.

### Plan per system

| System | H.264 video | AAC audio |
|---|---|---|
| Windows | Chromium's hardware decoder (D3D11). Works in CEF with only the build patch below ([CEF #3559](https://github.com/chromiumembedded/cef/issues/3559)) | Chromium's `MediaFoundationAudioDecoder`, which today only takes xHE-AAC. Cromite's patch shows the change: let it take all AAC and make FFmpeg refuse AAC ([Cromite #856](https://github.com/uazo/cromite/pull/856)) |
| macOS | Chromium's VideoToolbox decoder. Works with the build patch (CEF #3559) | Chromium's `AudioToolboxAudioDecoder`, which today only takes xHE-AAC. The same change as Windows. Untried by anyone found |
| Linux | VA-API, which Stardust already turns on when a driver is installed | No OS decoder exists. Build FFmpeg as a separate `libffmpeg.so` (`is_component_ffmpeg=true`) without the codecs, so a person or distribution can swap in one with them, as Opera and Vivaldi do. Or a Flatpak using the FFmpeg extension |

### Build settings

- `proprietary_codecs=true` with `ffmpeg_branding="Chromium"`, so the H.264 and AAC parsers are built but FFmpeg's decoders for them aren't. Cromite builds with `ffmpeg_branding="Chrome"` and only refuses the decoders at run time, which still ships them, so it isn't followed there.
- GN asserts that `proprietary_codecs` needs Chrome branding. CEF #3559 has a three-line patch to `media/BUILD.gn` that skips the assert for CEF builds.
- New flags, as in Cromite: one to send AAC to the platform decoder and one to keep H.264 off FFmpeg. Cromite's patch is GPL-2.0, so Stardust's must be written fresh, not copied. Stardust's own code is MPL-2.0.
- Unknown until built: whether FFmpeg's MP4 demuxer still passes AAC tracks with Chromium branding. In CEF #3559's test it skipped them, possibly only because no decoder accepted them.

### Limits of this way

- No software fallback: a computer without a hardware H.264 decoder, such as a virtual machine or an old graphics card, plays no H.264.
- Windows needs its AAC decoder (`msauddecmft.dll`), missing from some "N" editions.
- Linux without VA-API, or without a swapped-in FFmpeg, plays neither.

### The build

The patch and build script are in [cef-build/](../cef-build/README.md). The patch applies to Chromium 156.0.8078.48, GN accepts the settings, and the one patched file Linux builds compiles. Nothing has been built in full or run yet.

- CEF 156.0.14 (commit `4fa4ab5`, Chromium 156.0.8078.48, CEF branch 8078), matching the `cef` crate. Build with CEF's `automate-git.py --branch=8078 --checkout=4fa4ab5`, with the settings above in `GN_DEFINES`.
- Each system builds on its own kind of computer: Linux on Linux, Windows on Windows, macOS on a Mac.
- Needs at least 16 GB RAM, 32 GB or more recommended, about 200 GB of disk, and about 4 hours on 16 cores ([CEF build guide](https://chromiumembedded.github.io/cef/master_build_quick_start)). The Linux computer used so far has 4 cores and 7 GB RAM, which isn't enough.
- Stardust then uses it without code changes: `cef-rs` takes a local build from `CEF_PATH`, or downloads from `CEF_DOWNLOAD_URL`.
- Every CEF security release needs the build again, on all three systems, so it should become automatic.

## 8. Saved passwords: limits

In `crates/cef-host/src/client.rs` (`offer_to_save_login`, `fill_login`), `crates/cef-host/src/render.rs` and `crates/bridge/src/logins.rs`:

- **The real password store is untested** on every system. Tests used `--use-mock-keychain`, which keeps passwords in memory. On macOS, rebuilt ad-hoc-signed builds may make the Keychain ask permission again after each build.
- **Several logins for one site:** the key button fills the most recent one. Needs a chooser, for example in the menu under the toolbar.
- **Logins split over two pages,** and login forms inside frames, aren't handled.

## 9. Not for important accounts yet

What still stands between Stardust and real logins or banking, as of 2026-11-24:

1. **Security updates don't reach anyone** until the real signing key and a release server exist (section 4). This is the main reason.
2. **No Safe Browsing.** Chromium's malware and phishing checks need Google's service and aren't in CEF. Downloads aren't checked either.
3. **Google sign-in** loads, but whether Google lets a CEF browser finish signing in wasn't tested, because it needs a real account. Sign-in popups that send a result back to the page that opened them work.

Fixed on 2026-11-24: a site with a bad certificate now shows "This connection isn't secure" with no way past it. Before, the tab stayed blank and the address bar showed a padlock. The padlock no longer shows for a page stopped by a certificate error. It still goes by the address alone otherwise, so it doesn't reflect mixed content.

## 10. Permission prompts: limits

- **Location** uses the same path as the camera, but was never tested end to end, to avoid macOS's real location prompt.
- **Notifications** are refused: Alloy-style CEF can't show them.
- **Sharing the screen** is refused: it needs a picker.
- **Escape** on a prompt counts as Block and is remembered. Chrome treats closing a prompt as no answer.

## 11. Extensions

**Updated 2026-11-17: extensions now work.** An earlier version of this section said they couldn't without moving to CEF's Chrome style. That was wrong: Chromium's extension system runs in Alloy style too, and loads any folder given to `--load-extension`. Stardust has an extensions hub, a toolbar menu, pinned buttons and popups. See [milestone-5.md](milestone-5.md#extensions) for what works and what doesn't.

What's left:

1. **Decide on the Chrome Web Store.** Most people find extensions there. Installing from it means fetching `.crx` files from Google's update service. The unpacking is already built (`crates/extensions`). Whether Google's terms allow it for a browser other than Chrome is the open question. Brave, Vivaldi and Edge do it, with their own arrangements.
2. **Tabs and windows APIs.** `chrome.tabs.query` and `chrome.windows.getAll` return nothing, because Alloy has no Chrome tab model. Extensions built around "the current tab" mostly don't work. A bridge isn't possible from CEF's API, and would likely mean patching Chromium.
3. **Extensions without a restart.** Chromium only loads extensions as it starts, so each change needs a restart.
4. **Fit the popup to its content** instead of the fixed 380x520.
5. **Extension updates**, which the Web Store would normally handle.
6. Test on Linux and Windows.

## 12. Looked at, but not by a person

**Updated 2026-11-17.** Stardust's own pages can now be looked at without the screen: `python3 scripts/cdp.py shot <page> file.png` saves a picture of the toolbar, a menu, Settings or the extensions hub, which was how Milestone 5 was checked. What can't be seen that way is the native window frame. Have a person check: the window's close, minimize and zoom buttons (**minimize didn't register in tests**), the permission and save-password prompts, print and developer tools, and that nothing looks wrong at other screen sizes or text sizes.

Also: one unexplained "Google" tab appeared once during testing and didn't come back. See [milestone-5.md](milestone-5.md#a-stray-tab).

## 13. Workspaces

Built in [milestone-6.md](milestone-6.md). What's left:

1. **Extensions only run in Personal.** Chromium loads them into the original profile only.
2. **Clear browsing data** in Settings clears the Personal profile's cookies and cache only, not a workspace's.
3. **History and bookmarks are shared.** Decide whether a workspace should have its own.
4. Memory cost of many workspaces open at once, and the untested cases listed in the milestone.
5. Icons for Windows (`.ico`) and Linux.
6. **Split view:** test clicking a pane to focus it, a draggable divider, splitting an existing tab, saving the split across restarts, a shortcut key. See the milestone.
7. **Side panel:** per-workspace notes (a decision), keeping it open between runs, resizing, inserting snippets into pages, search and export, and private windows sharing the notes. See the milestone.
8. **Command bar:** search bookmarks and history in it, order by recent use, more commands, and a keyboard test of the shortcut. See the milestone.
9. **Web clipper:** folders and tags, search, saving to a notes folder or Obsidian, clipping an offline copy or a screenshot, frames, and finding the cause of a panel that closed once. See the milestone.
10. **Screenshots:** copy on Windows and Linux, dragging out a region, moving marks, a shortcut, and the block effect on real text. See the milestone.
11. **Colour picker:** `--force-color-profile=srgb` is kept by decision (it changes how everything is drawn on wide-gamut screens, so check video and photos look right); show the font actually drawn, more colour formats, and test on Windows and Linux. See the milestone.
12. **Asset library:** folders or collections, bulk tag and delete, sorting, export, saving pictures from pages, a test with colours by hand, and speed with a large library. See the milestone.
13. **Menus and right-click:** Windows and Linux versions, the missing Chrome items listed in the milestone (translate, cast, recent bookmarks and closed tabs, pin tab, task manager), hand tests of the untested items, and the duplicate Full Screen item. Old notes, to-dos and snippets code could be deleted.
14. **Welcome page:** try it on Windows and Linux (download the builds from the CI run), see the import step with a real second browser, and consider a default-browser step and a short tour.
15. **Vertical tabs:** groups, search, pinned tabs, resizing by dragging, keyboard navigation, and a real-mouse test of drag reordering. See the milestone.

## How to test on macOS

- Build with `cargo xtask bundle`, then run `./dist/stardust.app/Contents/MacOS/stardust`.
- **Keep tests out of real data:** set `HOME` to an empty folder, and pass `--use-mock-keychain`. Without it, macOS shows a "Keychain Not Found" dialog that holds up page loads, and saved passwords would go to the real Keychain.
- **Fake camera and microphone:** `--use-fake-device-for-media-stream`.
- **Drive the browser:** start it with `--remote-debugging-port=9333`, then use `python3 scripts/cdp.py` to run JavaScript in a page by title, or to send key presses. The toolbar page is titled `Toolbar`, the menu `Menu`, and Settings `Settings`. The toolbar's `window.browserBridge.send(JSON.stringify({type: "openPage", page: "settings"}))` opens Settings in a new tab.
- **Local test servers** must handle connections in parallel, such as Python's `ThreadingHTTPServer`. Chromium opens connections early and leaves them idle, which stalls a server that handles one at a time.
- **Don't pass `--user-data-dir` to Stardust:** it conflicts with the profile path Stardust sets, and pages stop loading.
- **Close old copies before relaunching a rebuilt app:** macOS kills a launch with exit code 137 if an older copy from the same path is still running.
- **`pkill -f <word>`** also matches the shell running the command if its text contains the word.
- **stderr** from the browser doesn't show up when it's started from a sandboxed shell. Write debug output to a file.
- **Disk:** a CEF build cache is about 7 GB in `target/`.
