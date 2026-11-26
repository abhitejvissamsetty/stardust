# Stardust

By Ambiqa. An open-source, lightweight browser written in Rust on top of Chromium, through the
[Chromium Embedded Framework](https://github.com/chromiumembedded/cef) and
[`cef-rs`](https://github.com/tauri-apps/cef-rs).

**Status: early prototype.** It has tabs, windows, private windows, downloads, bookmarks, history, settings, find-in-page, session restore, import, saved passwords, permission prompts, Chrome extensions and signed updates. It has been run on Linux and macOS; Windows code exists but has never been run. See the results for [Milestone 0](docs/milestone-0.md), [1](docs/milestone-1.md), [2](docs/milestone-2.md), [3](docs/milestone-3.md), [4](docs/milestone-4.md) and [5](docs/milestone-5.md), the [roadmap](docs/roadmap.md), and what's [left to do](docs/remaining-work.md).
Don't use it for logins or banking yet: it can update itself, but there's no release server, so it won't receive security fixes.

## Build and run

Needs Rust, CMake and Ninja. The first build downloads CEF (about 330 MB) for each build type.

```bash
cargo xtask bundle --release
./dist/stardust --url=https://example.com/
```

## Layout

- `crates/bridge`: toolbar messages, address parsing and shortcuts, with no CEF dependency
- `crates/extensions`: reads, unpacks and lists Chrome extensions
- `crates/import`: reads bookmarks and history from other browsers
- `crates/update`: checks, downloads and installs signed updates
- `crates/perf`: measures a built browser against budgets, including 20 tabs against Chrome
- `crates/storage`: bookmarks, history and settings in SQLite
- `crates/cef-host`: starts CEF, builds the window and handles its callbacks
- `crates/shell`: the application binary
- `ui`: the toolbar page and [design tokens](docs/design.md)
- `xtask`: builds the bundle in `dist/`, and makes signed releases ([how](docs/milestone-4.md#making-a-release))
- `scripts/measure.sh`: measures RAM and CPU on a page
- `scripts/cdp.py`: drives a running browser for testing, and takes pictures of its own pages, through Chromium's debugging protocol

## License

[MPL-2.0](LICENSE). CEF and Chromium have their own licenses, listed in `CREDITS.html` in the bundle. The official builds Ambiqa distributes, and the Stardust name and logo, are covered by the [End User Licence Agreement](legal/EULA.txt), which the installer asks people to accept. It defers to the open-source licences for the code they cover.
