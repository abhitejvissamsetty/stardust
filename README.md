# rust-browser

An open-source, lightweight browser written in Rust on top of Chromium, through the
[Chromium Embedded Framework](https://github.com/chromiumembedded/cef) and
[`cef-rs`](https://github.com/tauri-apps/cef-rs).

**Status: early prototype.** It has tabs, windows, downloads, bookmarks, history, settings, find-in-page, session restore and import. It has only been run on Linux; Windows and macOS code exists but has never been run. See the [Milestone 0](docs/milestone-0.md) and [Milestone 1](docs/milestone-1.md) results.
Don't use it for logins or banking: it has no update mechanism, so it won't receive security fixes.

## Build and run

Needs Rust, CMake and Ninja. The first build downloads CEF (about 330 MB) for each build type.

```bash
cargo xtask bundle --release
./dist/rust_browser --url=https://example.com/
```

## Layout

- `crates/bridge`: toolbar messages, address parsing and shortcuts, with no CEF dependency
- `crates/import`: reads bookmarks and history from other browsers
- `crates/perf`: measures a built browser against budgets
- `crates/storage`: bookmarks, history and settings in SQLite
- `crates/cef-host`: starts CEF, builds the window and handles its callbacks
- `crates/shell`: the application binary
- `ui`: the toolbar page and [design tokens](docs/design.md)
- `xtask`: builds the bundle in `dist/`
- `scripts/measure.sh`: measures RAM and CPU on a page

## License

[MPL-2.0](LICENSE). CEF and Chromium have their own licenses, listed in `CREDITS.html` in the bundle.
