# Roadmap

**Draft.** The original plan isn't in the repo. This scope for Milestones 3 and 4 is drawn from what the Milestone 0 to 2 docs leave for later.

## Where things stand

| Milestone | Theme | Status |
|---|---|---|
| 0 | CEF runs, with measurements against Chrome | Done on Linux |
| 1 | One window with a toolbar | Done on Linux |
| 2 | Daily-driver features | Done in code. The week of daily use is still to do |
| 3 | Memory, speed and everyday polish | Built and tested on macOS. Memory goal not met yet: [results](milestone-3.md) |
| 4 | Safe to recommend | Not started |

## Milestone 3: memory, speed and everyday polish

**Goal:** with 20 tabs open, Stardust uses less memory than Chrome, and the gaps found in Milestone 2 that people hit every day are closed.

Many pieces below turned out to exist already from the end of Milestone 2. [Milestone 3 progress](milestone-3.md) says which.

### Must have

| Piece | Why |
|---|---|
| Tab discarding | 8 real tabs used about 1,465 MB. Background tabs should unload after a while, or under memory pressure, and reload when selected. Pinned tabs, tabs playing audio and tabs with unsaved form text are kept |
| 20-tab test against Chrome | Added to `perf`, so the budget compares with Chrome instead of fixed limits |
| Address bar suggestions | From history and bookmarks, ranked by visit count and how recent, with keyboard selection |
| Clear browsing data | Cookies, cache and history, with a time range. Also clears saved tab addresses |
| Restored tabs keep Back and Forward | Save each tab's navigation history, not just its current address |
| Tab strip scrolling | Tabs stop shrinking at a minimum width and the strip scrolls |
| Closing a tab returns to its opener | Matches what people expect from other browsers |

### Should have

| Piece | Why |
|---|---|
| Bookmark folders, editing and a bookmarks bar | Imports currently flatten folders |
| Download history that survives restart | The list is lost on close today |
| Ask where to save, and a download folder setting | Always uses Downloads today |
| Warn when quitting during a download | Downloads are cancelled with no warning today |
| Find-in-page match case, and closing on navigation | Known gaps from Milestone 2 |
| Keyboard tab reordering | Mouse only today |

### Done when

- The 20-tab test passes in `perf` and in CI.
- Each piece above is tested on Linux the same way as Milestone 2.
- Milestone 2's week of daily use has happened, so its findings can be fixed here.

## Milestone 4: safe to recommend

**Goal:** Stardust can be recommended to someone who isn't a contributor. Today the README warns against logins and banking, because there's no way to get security fixes.

### Must have

| Piece | Why |
|---|---|
| Updates | Check for, download and install new versions, so CEF security fixes reach people. Signed packages only |
| Windows and macOS run | Both are type-checked only. They need CI runs, signing and a person launching the app on each |
| Permission prompts | Camera, microphone, location and notifications are denied automatically today |
| Risky download warnings | Then an Open button for downloaded files can be added |
| Security review of the internal pages' bridge | Including the case of a compromised renderer, which hasn't been tested |

### Should have

| Piece | Why |
|---|---|
| Extension review | Decide whether extensions are possible with CEF's Alloy style, which draws no Chrome UI, and what supporting them would cost. A decision and a written report, not necessarily extensions |
| Password saving and form autofill | Not available in Alloy style, so this needs its own work |
| H.264 and AAC | Many sites need them. Needs a custom CEF build and legal advice on patent licensing first |
| Hardware video decoding on by default | Where a working driver is found |
| Encryption of saved cookies | Not reviewed yet |

### Done when

- An installed build updates itself to a newer signed build on all three systems.
- The README's warning against logins and banking can be removed.

## Open questions

- Should the codec work wait for legal advice, or move to a later milestone?
- Is extension support a goal, or is the review enough for now?
- Is macOS a priority? This would decide whether Milestone 4's platform work starts early.
