# scripts

## Overview

Shell scripts for the desktop app's build and measurement chores.

## Interface

- `bindings.sh` — writes the TypeScript bindings the webview is typed by into `src/ipc/generated`: the daemon's protocol (`muster-core`, feature `ts`) and the app's own contract (`src-tauri/src/contract.rs`), through each crate's `export_bindings` tests with `TS_RS_LARGE_INT=number`. Invoked as `pnpm bindings`.
- `measure-idle.sh` — measures the release app at rest (`ARC-25`): CPU while seen and idle, CPU while hidden (which must be zero), and memory, for the app and the WebKit processes it started. Run on a Mac after `pnpm tauri build --no-bundle`; `SECONDS_EACH` sets each period (60 s by default), `MUSTER_BIN` the `muster` binary (else the core's debug build).

## Invariants

- `bindings.sh` replaces `src/ipc/generated` wholesale; those files are never edited by hand. No CI job runs it or checks the folder (the desktop jobs are out of CI), so run it by hand after changing a Rust type the webview uses, and before a release.
- `measure-idle.sh` runs over a daemon of its own (a runtime folder under `/tmp`, data/config/worktrees in a temporary folder, notch and keychain off), never the person's.

## Side Effects

- `bindings.sh`: deletes and recreates `src/ipc/generated`; runs `cargo test` in the core workspace and in `src-tauri`.
- `measure-idle.sh`: creates and removes a sandbox (a git repo, a stand-in `codex`), starts the release app and a daemon, adds a project and starts two sessions, drives the window through System Events (`osascript`, which macOS may ask to allow once), and stops the daemon on exit.

## Verification

- No test runner. Invoke `pnpm bindings` (then `git diff src/ipc/generated` shows what changed), or `./scripts/measure-idle.sh` after a release build.
