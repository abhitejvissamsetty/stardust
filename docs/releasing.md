# Releasing Stardust

How a new version gets from this repository to everyone's installed Stardust, step by step.

## How updates work

- **Checks:** each install checks the update address 30 seconds after starting, then once a day. People can also click "Check for updates" in Settings.
- **The manifest:** the address serves `manifest.json` and `manifest.json.sig`. The manifest names the newest version and one package per platform, with each package's size and SHA-256.
- **Signatures:** an install believes a manifest only if it's signed by a key in [`crates/update/release-key.pub`](../crates/update/release-key.pub), and installs a package only if it matches the manifest. A tampered server, network or download can't install anything.
- **Installing:** a new version downloads in the background and installs the next time Stardust starts.
- **No downgrades:** Stardust never installs an older or equal version.

## Releasing step by step

A manifest's `rollout` is the percentage of installs that take the release on their own.
- **Groups:** each install picks a random group from 0 to 99 once, and takes a release when its group is below the rollout.
- **Widening:** raising the rollout re-signs the manifest; installs already given the release keep it.
- **Asking:** "Check for updates" in Settings ignores the rollout.

A typical release goes 5%, then 25% a day or two later if nothing breaks, then 100%.

## The signing key

The private key is `~/.stardust-keys/release.key` on the maintainer's computer, made on 2026-11-25. It never goes into the repository, CI or GitHub secrets.

- **Back it up** somewhere safe and offline, such as a password manager or an encrypted drive. Without it, no install can be updated again, except by people reinstalling by hand.
- **If it leaks,** anyone could sign an update for every install. Make a new key, ship a release signed by the old key that trusts both, then drop the old one.
- **Changing keys:** `release-key.pub` takes several keys, one a line, and a manifest signed by any of them is believed. To move to a new key, add its public half beside the old one, release that version signed by the old key, then sign later releases with the new key.

## Making a release

1. **Set the version** in the root `Cargo.toml`, such as `0.2.0`. Commit and push.
2. **Tag it,** and push the tag:

   ```bash
   git tag v0.2.0
   ```

   ```bash
   git push origin v0.2.0
   ```

   The [Release workflow](../.github/workflows/release.yml) then:
   - Builds Stardust on Linux, Windows and macOS.
   - Packages each build with `cargo xtask package`.
   - Attaches everything to a **draft** release on GitHub.
   - Signs nothing.
3. **Sign it** on this computer, for 5% of installs to start with:

   ```bash
   scripts/sign-release.sh v0.2.0 5
   ```

   This downloads the draft's `.package.json` files (one per platform), writes `manifest.json` and signs it with `cargo xtask sign`, then uploads both.
4. **Widen it** later, re-signing at a higher rollout, and publish the draft on the releases page when it reaches everyone:

   ```bash
   scripts/sign-release.sh v0.2.0 25
   ```

   ```bash
   scripts/sign-release.sh v0.2.0 100 --publish
   ```

## The update address

Builds look for updates at `STARDUST_UPDATE_URL`, fixed when they're built. CI takes it from the repository variable of the same name. `--update-url` overrides it, for testing.

**For now it's empty, so updates are off in released builds.**
- **Why:** the repository is private, and installs download without signing in. Putting a GitHub token in the browser would let anyone with a copy read the repository.
- **Meanwhile:** the GitHub release still holds every package, for downloading by hand.
- **To switch updates on:**
  - Serve the release's files from a place reachable without signing in, such as Stardust's own server.
  - Set the repository variable to the manifest's full address, such as `https://updates.example.com/stardust/manifest.json`. It must be HTTPS.
  - Builds from then on check that address.

  The files are the same everywhere, so a server only needs the release's `manifest.json`, `manifest.json.sig` and packages. Builds made before the variable was set never check for updates; their users reinstall once.

## Tested

On Linux on 2026-11-25, with the real key, a 0.1.1 package signed at 1% rollout, served from this computer:
- An install in group 50 wasn't offered it.
- An install in group 0 downloaded it.
- "Check for updates" in group 50 downloaded it too.
- A restart installed it.

The release workflow hasn't run yet; the first tag will be its test.

## Not done yet

- **Windows:** installing an update hasn't been tested. Windows locks a running program's files.
- **macOS:** releases aren't signed or notarized, and the macOS bundle is a debug build (a limit of `cef-rs`'s macOS bundler).
- **Installs in protected folders:** an install in a folder only an administrator can change, such as one for all users, can't update itself.
- **Package size:** each update is the whole browser, about 160 MB. There are no patch updates.
