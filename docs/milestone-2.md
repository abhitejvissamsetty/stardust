# Milestone 2 progress

Every planned feature now exists on Linux. **The milestone isn't finished:** its goal is a contributor using the browser as their daily browser for a week, which hasn't happened. Windows and macOS have never been run.

Tested on Linux with real keyboard and mouse input on a private display.

| Piece | Status |
|---|---|
| Tabs and tab strip with drag | **Done** |
| Popups as tabs | **Done** |
| Multiple windows | **Done** |
| Downloads | **Done** |
| Bookmarks, history and settings | **Done** |
| Saved logins and cookies | **Done** |
| Find-in-page | **Done** |
| Session restore | **Done** |
| Import from Chrome and Firefox | **Done** |
| Performance budget tool | **Done**, with the CI step written but never run |

## Tabs: what was tested

- **Creating:** Ctrl+T and the + button, with the address bar ready for typing.
- **Switching:** clicking a tab, Ctrl+1 to Ctrl+9, Ctrl+Tab and Ctrl+Shift+Tab.
- **Per-tab state:** each tab keeps its own history, URL, title and loading state, including a load continuing in the background.
- **Closing:** Ctrl+W and middle-click, which also free the tab's processes.
- **Quitting:** Ctrl+Q exits with code 0 with many tabs open.
- **Reordering:** dragging a tab to a new position.
- **Popups:** a clicked popup opens as a tab, sends a message to the page that opened it, and closes itself, which is the flow many sign-ins use. Popups without a click are blocked.

The tab model has unit tests in `crates/bridge/src/tabs.rs`, and the project now has 75 tests in total.

## Downloads: what was tested

Tested against a local server and one real HTTPS download, saving to a temporary folder.

- **Integrity:** a 1 MB test file and a real GitHub zip, which involves a redirect, each matched the expected SHA-256 hash.
- **No overwriting:** repeat downloads become `report (1).tar.gz`, `report (2).tar.gz` and so on.
- **Hostile names:** `../../.bashrc` stayed inside the downloads folder.
- **Progress:** size, speed and a progress bar appear during the download.
- **Cancel:** removes the partial file.
- **Failed transfers:** one row marked Failed, even though Chromium retries them.
- **Links in a new tab:** the blank tab goes away, progress keeps updating, and its processes end with the download.
- **`download` attribute links:** work.
- **Remove and Clear:** take rows off the list and leave the files on disk.
- **Show in folder:** requested the right folder. The test used a stand-in, so a real file manager hasn't opened.
- **Quitting during a download:** exits cleanly and removes the partial file.

## Saved data: what was tested

Tested with a temporary profile, including a full browser restart.

- **Cookies:** persistent and session cookies both survived a restart. Before this work the browser kept no profile on disk, so every restart would have logged you out.
- **Bookmarks:** the star and Ctrl+D add and remove them, and the star is correct after a restart.
- **History:** visits are saved with titles and counts, and history can be searched. Failed loads stay out, and the previous page's entry isn't touched.
- **Pages:** `browser://history`, `browser://bookmarks` and `browser://settings` open by button, shortcut or typed address.
- **Settings:** the search engine and homepage are saved and used. `javascript:alert(1)` is rejected as a homepage.
- **Errors:** the address bar shows the failed URL, Back returns correctly, and Reload succeeds once the site is back.

## Multiple windows: what was tested

- **New windows:** Ctrl+N opens one with the address bar ready, and each has its own tabs.
- **Closing one window:** the others keep running, and the session forgets the closed window.
- **Closing the last window:** its tabs reopen next time.
- **Quit:** Ctrl+Q closes every window and exits with code 0, and they all reopen after a restart.

## Import: what was tested

Tested with a fake home folder containing fixture Chrome and Firefox data.

- **Sources:** Chrome and Firefox profiles were found and imported through Settings. Chromium, Edge and Brave use the same code with different folders, and weren't run.
- **Repeat imports:** add nothing and don't inflate visit counts. Your own bookmark titles are kept.
- **Filtering:** only web addresses come across, so `javascript:` bookmarks are dropped.
- **Originals:** the source database is copied first and left unchanged.
- **Temporary copies:** none were left behind.
- **Real Firefox format:** the table structure of a real, current Firefox database has every column used. Its contents weren't read, so a real import hasn't been run.

Passwords, cookies and payment data are never opened.

## Performance budgets

`cargo run --release -p perf -- dist` measures a built browser against limits and fails if any is exceeded. One run on the test laptop:

| Measurement | Result | Limit |
|---|---|---|
| Installed size | 372 MB | 450 MB |
| Startup to first request | 0.6 to 6 s | 15 s |
| RAM with one simple tab | 368 to 371 MB | 500 MB |
| Idle CPU | 0% | 5% |

The limits catch regressions, and don't yet express the plan's comparison with Chrome. The 20-tab test is planned for Milestone 3. Startup varied a lot between the two runs, so that limit is loose.

## How saved data is protected

Web pages see no `browserBridge` on normal pages, confirmed in testing. Internal pages use a limited command set, which the browser process accepts only from the main frame of a tab on an internal page's exact URL. That check hasn't been tested against a compromised renderer.

## Find-in-page: what was tested

- **Opening:** Ctrl+F opens the bar with its text field ready.
- **Moving between matches:** Enter, Shift+Enter and F3 move between matches, and the count updates.
- **No matches:** the field turns red and says so.
- **Closing:** Escape closes the bar and clears the highlights, and switching tabs closes it too.

## Session restore: what was tested

- **After quitting:** tabs, their order and the active tab all came back.
- **After a crash:** a `kill -9` lost nothing, including a tab opened just before it.
- **Lazy loading:** only the active tab loads at startup, so restoring 4 tabs ran 9 processes instead of 13. Others load when selected, including internal pages.
- **Closing an unloaded tab:** works without ever loading it.
- **Opening with a URL:** it opens as a new tab beside the restored ones.
- **Settings:** "When the browser starts" can reopen tabs or open the homepage, with reopening as the default. The homepage mode was tested by setting the value directly in the database, not by using the dropdown.
- **Damaged or edited sessions:** covered by unit tests. They come back empty, and unsafe URLs are dropped.

## Memory

8 tabs of real sites used about 1,465 MB, measured once and with no matching Chrome figure yet. Tab discarding, which should help most here, is planned for Milestone 3.

## Known limits

- **Windows:** tabs can't be dragged between windows, and each window has its own download bar. Each window's toolbar also costs an extra process.
- **Import:** folders are flattened, because bookmark folders don't exist yet. Up to 20,000 history entries come across, and a large import may briefly freeze the window.
- **Restored tabs lose Back and Forward:** only each tab's current address is saved. Scroll position and form text aren't restored either.
- **Saved tabs and privacy:** open tabs' addresses stay in the database after Clear all history, until those tabs close.
- **Closing the last tab:** it's treated like quitting, so that tab reopens at the next start.
- **Find options:** no match-case option, and the bar stays open if the tab navigates to a new page, with the old count.
- **Open dropdown list:** the settings search engine dropdown works, but one screenshot showed its open list as a black box on the virtual display. It needs checking on a real desktop.
- **Address bar suggestions:** typing doesn't suggest bookmarks or history yet.
- **Bookmark organization:** no folders, editing or bookmarks bar.
- **Clearing data:** Clear all covers history only, not cookies or the cache.
- **Saved data encryption:** bookmarks, history and cookies are stored as ordinary files in the profile, relying on OS account protection. Encryption of cookies hasn't been reviewed.
- **Long lists:** history and bookmark pages show at most 500 results.
- **Back and Forward:** returning to a page may not add to its visit count.
- **Open a downloaded file:** there's no button for it yet, only Show in folder. That's deliberate for now, because there's no warning for risky file types.
- **Malware checks:** downloads aren't scanned or checked against any blocklist.
- **Quitting during a download:** it's cancelled with no warning.
- **Download location:** always the Downloads folder, with no setting or Save As prompt.
- **Download history:** the list is lost when the browser closes.
- **Many downloads:** the bar cuts off older ones, with no scrolling.
- **Large and resumed downloads:** the largest file completed was 1 MB. Larger files were only tested part-way, and pause and resume don't exist.
- **Sign-in popups:** a popup opened after an asynchronous step can lose its click and be blocked. Real sign-in providers haven't been tested.
- **Pop-up size:** a requested popup size is ignored, because popups open as full tabs.
- **After closing a tab:** the neighbor shows next, not the tab that opened it.
- **Cancelled closes:** if a page's "Leave site?" prompt cancels closing the window, later tab closes may behave wrongly. This hasn't been tested.
- **Keyboard reordering:** tabs can only be reordered with a mouse.
- **Too many tabs:** tabs shrink to fit, with no scrolling yet.
- **Windows and macOS:** type-checked only, never run.
