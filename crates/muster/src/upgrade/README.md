# upgrade (submodules of `upgrade.rs`)

## Overview

The Mac app half of `muster upgrade` (`PRODUCT.md` §6a, `ARC-21`). The parent
[`../upgrade.rs`](../upgrade.rs) installs the latest release binary over the
running one, verified by SHA-256 and by `gh attestation verify`, and then
reloads the daemon. It calls `app::upgrade` to replace an installed
`Muster.app` with the release's universal app in the same checked way.

## Interface

- `App` (app.rs) — the outcome: `NotInstalled | NotInRelease | Current(version) | New { from, to, at }`.
- `upgrade` (app.rs, `pub(super)`) — downloads
  `muster-app-universal-apple-darwin.tar.gz` and its `.sha256`, checks them,
  unpacks the app beside the installed one, and swaps it when the versions
  differ.
- `installed_app` (app.rs) — finds `Muster.app` in `MUSTER_APP_DIR`, then
  `/Applications`, then `~/Applications`.
- `version_of` (app.rs) — `CFBundleShortVersionString` through
  `/usr/libexec/PlistBuddy`.
- `ARCHIVE` (app.rs) — the release asset's name.

## Invariants

- The app is checked like the program: always against its published SHA-256,
  and against the attestation (`super::say_attested`) where the GitHub CLI
  can, unless `MUSTER_SKIP_ATTESTATION` is set or the release is a stand-in;
  without the GitHub CLI, the checksum alone unless
  `MUSTER_REQUIRE_ATTESTATION` is set. A mismatch replaces nothing
  (`ARC-21`).
- A stand-in release (`MUSTER_RELEASES`, debug builds only) is installed only
  into `MUSTER_APP_DIR`, never into the person's own folders.
- The swap is two renames in the app's own folder; if the second fails, the
  old app is moved back. An open app keeps running from its original files.
- Off macOS, or with no app installed, it does nothing (`NotInstalled`). A
  release without the archive (404) gives `NotInRelease`.

## Side Effects

- Network: GET of the archive and its `.sha256` from the release.
- Processes: `/usr/libexec/PlistBuddy`, plus `tar` and `gh` through the
  parent's helpers.
- Files: a temp dir `.muster-app-upgrade-*` beside the app; the old app moves
  into it as `Muster.app.old` and is removed with it.
- stdout: `Downloading …`. Reads `MUSTER_APP_DIR` and `HOME`, and through the
  parent `MUSTER_RELEASES`, `MUSTER_SKIP_ATTESTATION` and
  `MUSTER_REQUIRE_ATTESTATION`.

## Verification

- Run: `cargo check -p muster`, then `cargo nextest run -p muster upgrade`
  (the real app upgrade is checked by hand before a release).
