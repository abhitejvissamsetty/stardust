# Creator-first roadmap

**Draft, written 2026-11-17.** A plan for turning Stardust from a general browser into one built for creators. It rests on what the code can do today, which is in [milestone-5.md](milestone-5.md) and [remaining-work.md](remaining-work.md). Sizes are rough: **S** is days, **M** is one to three weeks, **L** is a month or more, for a small team. Nothing here has been validated with real creators yet, and step one is to do that.

## The bet

Creators spend their working day in a browser, and they bolt on the same dozen extensions to make it work: research overlays (vidIQ, TubeBuddy, Keywords Everywhere), recorders (Loom, Screencastify), design helpers (Canva, ColorZilla, WhatFont), clippers (Notion, Obsidian), writing aids (Grammarly), and schedulers (Buffer). They juggle several channels and brands at once, keep dozens of tabs open per project, and lose time moving assets between tools.

**Stardust's pitch:** a browser where that setup is built in and works from the first minute, organised around projects and channels rather than a pile of tabs, and kept private and fast.

Why this can work where "a better Chrome" can't: general browsers compete on reach and defaults, and Stardust can't win there. A browser shaped for one job can.

## Who first

Start with **video and short-form creators** (YouTube, TikTok, Reels). Reasons:

- The most-recommended extensions are for them (vidIQ and TubeBuddy lead every list).
- Their needs are concrete and testable: screen and camera recording, thumbnails, uploads, analytics.
- They stress the hardest parts of a browser (video codecs, large uploads, capture), so serving them well proves the foundation.

Writers, designers, streamers and social managers share most of the workspace features and some of the tools, and come after. **Confirmed by the owner on 2026-11-17.** The private beta should still test it, and it can change before Phase 2, which is where it matters.

## Where Stardust is today

| Strength for creators | Gap for creators |
|---|---|
| Chrome extensions work: content scripts, content blocking, extension pages | The big commercial extensions are Chrome Web Store only, and the store's terms don't allow us in |
| Our own interface in web pages, so a side panel, vertical tabs or split view are things we can draw | H.264 and AAC video doesn't play, so many video sites and upload previews fail |
| Private windows, with a profile that lives in memory | Screen sharing and recording are refused: there's no picker for what to share |
| Camera, microphone and location prompts | Web notifications are refused, which hurts Studio, Slack and Discord |
| Tab unloading, to keep many tabs cheap | One profile only: no separate accounts for separate channels |
| Signed updates, built (no server yet) | Windows has never been run, and macOS isn't notarized |
| Passwords in the system store, import from Chrome and Firefox | Uses about 6% more memory than Chrome with 20 tabs |

## Principles

1. **Built-in beats bolt-on.** Anything creators install an extension for, and that we can do without the store, we do ourselves.
2. **Projects, not tabs.** The unit is a channel, brand or project.
3. **Never a hostage.** Creators' accounts, files and workspaces export in plain formats.
4. **Quiet by default.** No ads, no tracking that isn't clearly explained, no pop-ups of our own.
5. **Prove it on one wedge** before widening.

## Phase 0: usable every day, by a creator

**Goal:** a video creator can use Stardust for a full working week without going back to Chrome.

| Item | Why | Size | Depends on |
|---|---|---|---|
| Play H.264 and AAC | Without it, much of a creator's video work fails | L | Legal advice on patents; a build server for custom CEF. Two approaches are laid out in remaining-work.md |
| Screen and window capture, with a picker | Needed for any recorder, and for sharing a screen on calls | L | Research first: how much CEF supports, and whether a custom picker is needed |
| Web notifications | Studio, Slack, Discord, email | M | Research first: CEF's Alloy style can't show them natively |
| Multiple profiles | Separate accounts per channel or brand, each with its own cookies and logins | L | **Built as workspaces, see [milestone-6.md](milestone-6.md).** Logins are separate per workspace |
| Run it on Windows, then ship Windows builds | Most creators on Windows have never been able to try it | L | A Windows machine or CI |
| Release server, a real signing key, macOS signing and notarization | People can install it, and it updates itself | M | Decisions in remaining-work.md |
| Large uploads and drag-and-drop | Creators upload multi-gigabyte files constantly | S | Test, then fix what breaks |
| Finish the daily-use week | The test Milestone 2 still owes | S | A person |

**Exit:** five creators use it daily for two weeks with no blocking problem.

## Phase 1: the creator workspace

**Goal:** organise work by project, and make a day with many tabs calm.

| Item | Why | Size | Depends on |
|---|---|---|---|
| Workspaces: a named set of tabs, with its own profile, switchable in one click | The core idea: one workspace per channel or project | L | **First version built, see [milestone-6.md](milestone-6.md).** Logins are separate; extensions run in Personal only |
| Vertical tab sidebar, with groups and search | Dozens of tabs per project | M | **Sidebar built, see [milestone-6.md](milestone-6.md).** Groups and search aren't |
| Split view: two pages side by side | Research next to a script or a dashboard | M | **First version built, see [milestone-6.md](milestone-6.md).** Resizable divider, splitting an existing tab and saving it aren't |
| Side panel: notes, a to-do list and a clipboard of snippets, beside any page | Work without leaving the page | M | **First version built, see [milestone-6.md](milestone-6.md).** Notes aren't per workspace yet |
| Command bar (a quick switcher for tabs, workspaces and tools) | Speed | M | **First version built, see [milestone-6.md](milestone-6.md).** Doesn't search history or bookmarks, or learn from use |
| Saved sessions, and reopening a workspace as it was | Resume a project | S | Workspaces |

**Exit:** a creator can run three channels from three workspaces without mixing logins, and say so unprompted.

### Private beta, after Phase 1

Invite 20 to 50 creators. Their feedback decides Phases 2 and 3, so don't skip it.

## Phase 2: built-in creator tools

Build these as part of the browser. Many can be done with scripts the browser injects into the current page, which doesn't need the tabs API that extensions lack here. Pick by what the beta asks for, in roughly this order of likely value:

| Tool | Replaces | Size | Notes |
|---|---|---|---|
| Screen and camera recorder, with a webcam bubble | Loom, Screencastify | L | Needs capture from Phase 0 |
| Screenshot and annotate, full page or region | Several | M | **Built for the visible area and the whole page, see [milestone-6.md](milestone-6.md).** Region drag and copy off macOS aren't |
| Colour picker and font identifier | ColorZilla, WhatFont | S | **Built, see [milestone-6.md](milestone-6.md).** Needed the colour-profile change described there |
| Web clipper to a folder of notes, Notion or Obsidian | Clippers | M | **First version built, see [milestone-6.md](milestone-6.md).** Markdown copy only; no folders, tags or direct export yet |
| Thumbnail preview: how a title and image look in search and feeds | Part of vidIQ, TubeBuddy | M | |
| Quick tools menu: resize, convert and compress an image or short video | Many small sites | M | |
| Asset library: everything you've clipped, with tags and search | Folders of screenshots | L | |

**Exit:** the beta creators drop at least two extensions each, because the built-in version is enough.

## Phase 3: add-ons your way

The store stays closed to us, so build a route that doesn't need it.

| Item | Why | Size |
|---|---|---|
| A short curated catalogue of open-source extensions that work here, tested (SponsorBlock, Dark Reader, Video Speed Controller, Bitwarden, uBlock Origin Lite and similar) | Immediate value, with no store | M |
| Approach the makers of vidIQ, TubeBuddy and other creator tools: ask them to publish a direct download we can list, or a Stardust-specific build | These are what creators ask for, and partnerships may be the only legitimate route | M, and mostly conversation |
| Ask Google for the written agreement its terms require for store access | If granted, it unlocks everything | S to ask, unknown to answer |
| Extension updates, and a restart-free load if CEF allows it | Quality of life | M |
| Make more extensions work by providing the tab and window information they expect | Many extensions assume it | L, and may need changes to Chromium itself |

## Phase 4: public launch

| Item | Size |
|---|---|
| Website, download pages, an installer for each system, and a plain privacy policy | M |
| Onboarding: import from Chrome, pick your platform, make your first workspace | M |
| Crash reporting that's opt-in and explained | M |
| A help centre, and a place for creators to share workspaces and tips | M |
| Launch with creator partners and one or two well-known channels | M, and mostly outreach |
| Check the name "Stardust" for trademark conflicts before launch | S |

**Exit:** public downloads, with a way for people to ask for help and tell you what's wrong.

## Phase 5: grow

Decide among these with real usage in hand.

- **Sync** of workspaces, bookmarks, passwords and notes across a creator's computers, end-to-end encrypted. This needs servers and is the biggest cost. **L**.
- **Pro tier** for sync, cloud recording storage and heavier tools, with the core staying free. Needs a pricing decision.
- **An assistant** for titles, descriptions, scripts and clip ideas, with the person's own API key first. Costs and privacy need care.
- **Analytics across platforms** in one panel, through the platforms' own sign-in and APIs. Needs each platform's terms checked.
- **Teams:** shared workspaces for a creator and an editor or manager.
- **Widen the audience:** writers, designers, streamers.

## Decisions needed from you

1. ~~**The first audience**~~ Decided: video and short-form creators.
2. **H.264 and AAC:** the approach, and legal advice, since this blocks Phase 0.
3. **Money:** free and open, a paid tier, or something else? Affects Phases 4 and 5, and the licence (it's MPL-2.0 today).
4. **Team and budget:** who builds, and how much can be spent on signing certificates, a build server and later servers?
5. **Google:** whether to approach them about the store.

## Risks

- **Codecs and capture are hard, and may take longer than anything else.** Both gate the wedge. Start research now.
- **Creators won't switch for features alone.** Their logins, extensions and habits are in Chrome. Import, profiles and the built-in tools have to make switching painless.
- **The store.** Without it, the extension story is thin. Phases 2 and 3 have to cover that gap.
- **Platform terms.** Anything that touches YouTube, TikTok or Instagram data, or automates posting, must follow their terms. Read them before building.
- **A small team maintaining a browser.** Chromium security updates arrive every few weeks, and each CEF update must be tested and released. This never stops.
- **Memory and speed.** Stardust is still above Chrome on memory. Creators on laptops notice.

## Not now

Mobile apps, a store of our own, social features, a built-in video editor, and anything that posts for creators automatically. Each is a product of its own.

## How to know it's working

- Days per week a beta creator uses Stardust as their main browser.
- Extensions each creator removes after using the built-in tools.
- Time to get a new creator from download to their first workspace.
- Crashes and freezes per hour of use.
- Whether creators recommend it to another creator, unprompted.

## The next 30 days

1. Talk to 10 creators in the chosen audience. Watch them work. Ask what they'd give up Chrome for. This may change the order of everything above.
2. Start the three research spikes: H.264 and AAC, screen capture, and notifications. Each should end in a written "possible, how, how long".
3. Get Stardust running on Windows.
4. Build profiles, since workspaces, the private beta and several tools depend on them.
5. Finish the daily-use week on macOS and fix what it finds.
