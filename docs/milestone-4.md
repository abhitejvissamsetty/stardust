# Milestone 4 progress

Goal: Stardust can be recommended to someone who isn't a contributor.

Built and tested on macOS (Apple Silicon) on 2026-11-06. Tests drove the browser with real key presses and clicks through Chromium's debugging protocol, and checked results in its pages and saved data. Nothing on screen was looked at.

## Status

| Piece | Status |
|---|---|
| Updates | **Done and tested end to end on macOS.** Not run on Linux or Windows. There's no release server yet |
| Windows and macOS run | **macOS runs** (since Milestone 3). **Windows has never been run**: there was no Windows computer to test on |
| Permission prompts | **Done** for camera and microphone, tested. Location uses the same path, untested |
| Risky download warnings | **Already done** before this milestone: risky types need confirming, and downloads have an Open button |
| Security review of the internal pages' bridge | **Done**: one real hole found and fixed, see below |
| Password saving and filling | **Done**, tested with a stand-in password store |
| Cookie encryption review | **Done**: cookies are encrypted, see below |
| Hardware video decoding | **On by default on macOS**, checked. Turned on automatically on Linux where a VA-API driver is installed, untested |
| Extension review | **Done, but its answer was wrong and has been corrected**: extensions work, see [milestone-5.md](milestone-5.md) |
| H.264 and AAC | **Not started.** Needs legal advice on patent licensing first |

The milestone's "done when" isn't met yet. Updates work on macOS only, and the README's warning against logins and banking stays until there's a release server and a real signing key.

## Updates

A release server holds `manifest.json`, which names the newest version and a package for each platform with its size and SHA-256, and `manifest.json.sig`, an Ed25519 signature of it.

- **Checking:** 30 seconds after starting, then once a day, Stardust fetches the manifest. It believes it only if it's signed by the key built into `crates/update/release-key.pub`, and only takes versions newer than its own.
- **Downloading:** the package must match the signed size and SHA-256. It's unpacked into a staging folder in Stardust's data folder. Links inside it can't point out of the package.
- **Installing:** at the next start, before Chromium loads, the staged version is swapped in for the installed one, and Stardust starts again from it. The previous version is kept as `<name>.old` until the next start, so a failed swap can put it back.
- **Addresses:** updates come over HTTPS. Plain HTTP is only allowed to this computer, for testing.
- **Settings:** an Updates section shows the version and what the updater is doing, with Check now, and Restart to update once a version is ready.

The server address is built in when a release is made, with the `STARDUST_UPDATE_URL` environment variable, or given with `--update-url` for testing. Without one, updates are off and Settings says so.

### Making a release

```bash
cargo xtask keygen ~/.stardust-keys/release.key
cargo xtask bundle --release
cargo xtask release --key ~/.stardust-keys/release.key --out releases
```

Releases are now made in two steps, `cargo xtask package` and `cargo xtask sign`. See [releasing.md](releasing.md).

`keygen` writes the private key outside the repository, readable only by you, and puts the public key in `crates/update/release-key.pub`. Anyone with the private key can ship updates to every Stardust, so keep it safe. Run `release` on each platform with the same `--out` folder, and it adds that platform's package to the same signed manifest. Then upload the folder to the server.

The public key in the repository now is a **test key**, kept at `~/.stardust-keys/test-release.key` on the machine that made it. Make the real key before the first release, and commit its public half.

### Tested

On macOS: version 0.1.1 was built and released with the test key, into a local folder served at `http://127.0.0.1`. A separate installed copy of 0.1.0 was started pointed at it.

- It fetched the manifest and signature, downloaded the 178 MB package, and staged it.
- Settings showed "Version 0.1.0. Version 0.1.1 is ready."
- Restart to update closed it, installed 0.1.1, kept the old copy as `stardust.app.old`, and started 0.1.1.

Unit tests cover refusing a changed manifest, another key's signature, a package with the wrong size or checksum, a link out of the package, older versions, and plain HTTP to other computers.

### Limits

- **No release server, and a test key.** Both are needed before updates reach anyone.
- **Install folders must be writable** by the person running Stardust. An app installed for all users, where only an administrator can write, can't update itself yet.
- **macOS signing:** an update replaces the app with whatever is in the package. Releases should be signed and notarized, which isn't set up.
- **Restarting** after an update doesn't pass on the command line it was started with.
- **Linux and Windows:** the same code, never run.

## Permission prompts

When a site asks for the camera, microphone or location, the dialog bar shows the site and what it wants, with Block and Allow. The answer is saved for that site, and the next request is answered without asking. Settings lists each site's answers, with Reset.

- A request from a tab out of view, or while the bar is asking something else, is refused and not remembered.
- Switching tabs with a prompt showing dismisses it without remembering anything.
- One blocked permission blocks a request that also asks for others.
- Sharing the screen is refused, because it needs a picker for what to share. Notifications and everything else are refused too: Alloy-style CEF can't show notifications.
- On macOS, the app now tells the system why it wants the camera, microphone and location. Without the location text, macOS refuses location outright.

Tested with Chromium's fake camera, so the real camera was never used: Block refused the page and was remembered, Reset in Settings cleared it, and Allow gave the page a video track and was remembered.

## Security review: the internal pages' bridge

Internal pages, such as Settings and History, are `data:` URLs with a bridge to the browser. Any page's renderer could send the bridge's message, so the browser decides whom to believe.

**Sound:** the browser acts on internal page commands only from the main frame of a tab whose committed URL is exactly one of its own pages. It reads that URL itself, so a page can't claim it. A tab can't otherwise show a `data:` page: Chromium blocks web pages from opening one at the top level, and the address bar turns typed `data:` and `javascript:` addresses into searches. An internal page in a frame gets no bridge, and its messages would be refused anyway.

**Fixed: the toolbar would load any `data:` page.** The toolbar's navigation filter allowed any `data:text/html;base64,` address, and the browser trusts every message from the toolbar. A site could offer a link to such an address and, if it was dragged onto the toolbar, its page would run as the toolbar, able to send any toolbar command, including opening a downloaded file without the risky-file warning. The toolbar now loads only its own exact page, and the menu under it loads nothing. Tested: moving the toolbar to another `data:` page was refused.

**Also tightened:** the toolbar may now open no window other than its own menu, and the menu has no bridge of its own.

**Checked:** the internal pages and toolbar never put text from websites, such as titles and addresses, into HTML. They set it as text. The only HTML they set is their own icons.

**Not tested:** a compromised renderer. The browser's checks don't trust the renderer, and rely on Chromium's site isolation keeping each site in its own process, but no attack was attempted.

## Password saving and filling

When a login form is sent, the dialog bar asks "Save the password for ann?", with Save or Never. If that username is saved with another password, it asks to update it. Never stops asking on that site.

- **Where passwords go:** the system's password store: the Keychain on macOS, Credential Manager on Windows and the Secret Service on Linux. Stardust's database only holds each login's site, username and when it was last used.
- **Which pages:** HTTPS pages, and plain HTTP to this computer for developing sites. A password is never saved from an unencrypted page.
- **Filling:** a key button appears in the toolbar on a site with saved logins. Clicking it fills the most recently used login. Nothing fills without that click, so a page or a script injected into it can't get a password on its own.
- **Checks:** the fill goes to the tab's main frame only. The page's own process checks again that the frame is still that site before filling, in case it moved on.
- **The page's view:** a small script in each web page notices logins being sent. It's handed its reporting function directly, so the page's own scripts can't see or call it.
- **Settings:** lists saved logins with Remove, and sites set to Never with Allow saving. Passwords are never shown there.

Tested with `--use-mock-keychain`, which keeps passwords in memory so nothing was written to the real Keychain: the save prompt appeared after logging in, even though the page moved on; the key button filled the username and password; Remove in Settings hid the key button; and Never was remembered.

### Limits

- **The real password store is untested** on every system.
- **One login per click:** with several logins for a site, the key button fills the most recently used one. There's no list to choose from yet.
- **Simple forms only:** the script fills the first password field and the text field before it. Logins split over two pages, or inside frames, aren't handled.
- **Pages can read filled values,** as in every browser.

## Cookie encryption review

Checked on macOS with a page that set a cookie: in the profile's `Cookies` database the cookie's plain value is empty, its encrypted value starts with Chromium's `v10` marker, and the cookie's text appears nowhere in the file.

Chromium encrypts cookies with a key from the system:

- **macOS:** a key in the Keychain. With `--use-mock-keychain`, as tests run, it's a fixed stand-in key instead. Never ship with that switch.
- **Windows:** a key protected with the user's Windows login (DPAPI). Not checked.
- **Linux:** a key in the desktop's keyring (GNOME Keyring or KWallet) when one is running. Without one, Chromium falls back to a fixed key that only hides cookies from casual reading. Not checked.

Stardust's own database, with history, bookmarks, settings and saved login names, isn't encrypted. It relies on the system keeping other users out of the profile folder.

## Hardware video decoding

- **macOS:** `chrome://gpu` reports video decode and encode as hardware accelerated, with no changes needed.
- **Linux:** when a VA-API driver is installed (any `*_drv_video.so` where `libva` looks, or in `LIBVA_DRIVERS_PATH`), Stardust turns on `AcceleratedVideoDecodeLinuxGL` and `VaapiVideoDecoder`, keeping any features already given. It doesn't pass `--ignore-gpu-blocklist`, so GPUs Chromium knows to be faulty stay on software decoding. Untested beyond unit tests.
- **Windows:** Chromium's default. Not checked.

## Extension review

> **Corrected on 2026-11-17.** The conclusion below was wrong. CEF's API can't load extensions, but Chromium's own extension system is still there in Alloy style and loads any folder given to `--load-extension`. Stardust now supports Chrome extensions without moving to Chrome style: see [milestone-5.md](milestone-5.md#extensions). The original text is kept for the record.

**Decision at the time: no extensions with the current design.**

CEF 156, which Stardust uses, has no extension support for Alloy-style browsers: its API has no way to load or manage extensions. Chrome extensions only work in CEF's Chrome-style browsers, and every browser in a window must share one style.

Supporting extensions would mean moving Stardust to Chrome style. That brings Chrome's own extension, permission and password features, but also parts of Chrome's own interface, and it would change most of how Stardust's window is built. It's a large decision about what Stardust is, not a task, and isn't recommended now.

## Tests

All 136 unit tests pass on macOS: 86 in `bridge`, 25 in `storage`, 10 in `update`, 7 in `import`, 6 in `perf` and 2 in `cef-host`. Clippy and `cargo fmt` are clean.
