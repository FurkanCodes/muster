# src (muster-desktop)

## Overview

The Rust side of Muster's Tauri desktop app. The app is one more client of the
daemon, like the terminal UI and the notch (`ARC-22`, `ARC-24`). `main.rs` only
calls `muster_desktop::run()`. In `lib.rs`, `run()` adds the dialog plugin (and
the WebDriver plugin in `e2e` builds), then sets up:

- `muster_core::Paths`, from the environment and the uid;
- `DaemonLog` and `Kept` (`window.json`);
- the `Link`, whose task is held in `LinkTask`.

It then registers the commands. The webview never works state out:
`link` holds the daemon connection and the model, and pushes `StateMessage`
patches on a Tauri `Channel`. Terminal output travels as raw binary frames.
The other modules wrap the outside world.

## Interface

- `run` (lib.rs) — the Tauri entry; exits 1 on a setup error. `LinkTask` aborts the link task on drop.
- `link_open`, `command`, `attach`, `detach`, `input`, `resize`, `watch`, `looking`, `colours`, `try_now` (commands.rs) — thin wrappers over `Link`. `link_open` opens the state channel; a `Reveal` unminimises, shows and focuses the `main` window.
- `StateMessage`, `LinkState`, `Patch`, `Gone` (what `Patch.gone` removes), `Derived`, `Row`, `Counts`, `TermEvent`, `BridgeError` (contract.rs) — the app's own contract with the webview, exported to TypeScript by ts-rs. `BridgeError` is `Unreachable`, `Refused{code,message}` or `Invalid{message}`.
- `frame` (contract.rs) — binary frame layout `[kind u8][seq u64 LE][payload]`; kinds OUTPUT 0, REPLAY 1, REPLAY_RESET 2, SCREEN 3, EVENT 9.
- `Link` (with `is_connected`), `Options`, `StateSink`, `FrameSink`, `Looking`, `Start`, `RETRY_EVERY`, `START_WAIT`, `START_AGAIN_AFTER`, `PATCH_EVERY` (link.rs) — the daemon connection; its submodules are in [`./link/`](./link/README.md).
- `pick_folder`, `PickedFolder` (folders.rs) — the Mac's folder panel as a sheet opened at `HOME` (`Chosen`, `Cancelled`, `Unavailable` in `e2e` builds); the browser stand-in is in [`./folders/`](./folders/README.md).
- `show_daemon_log` (daemon_log.rs) — opens `runtime_dir/daemon.log` with `open`.
- `open_ticket_url` (ticket_url.rs) — opens an http(s) URL with a host, and nothing else.
- `hear` (sounds.rs) — plays a Mac system sound or `~/Library/Sounds/<name>` with `afplay`.
- `restart_in_place`, `open_notification_settings`, `edit_settings_file` (settings_actions.rs) — `muster reload`, the System Settings notifications pane, `settings.toml` in the text editor.
- `kept`, `keep`, `Kept` (kept.rs) — the window's own key/value state in `window.json`.
- `e2e_command_key`, `recorded` (e2e.rs) — `e2e` builds only: post ⌘Q/⌘W to the app; record outside actions instead of doing them.

## Invariants

- `ARC-22`/`ARC-24`: the webview draws and never works state out; Rust sends whole-entity patches plus derived data.
- `ARC-25`: terminal output never goes through JSON; patches are at least one frame (16 ms) apart.
- `ARC-33`: rows follow the terminal UI's order: a space heading, then its projects, workspaces and sessions, then its terminals in no project.
- `ARC-13`: the link task is aborted as the app ends. Every wait has a deadline: 5 s for `open`, 45 s for `muster reload`, 30 s for a sound.
- `PRD-09`: the daemon and every session keep running after the app quits.
- `ARC-24`: window state is kept locally and the daemon is never told. A shipped app never reads a folder itself. Settings actions run only on the person's word and take no path from the webview.
- `PRD-07`/`PRD-10`: only a ticket's explicit browser action opens anything, and only over http/https.
- Every outside call passes its path or URL as a single argv argument, never shell code. A sound name with `/`, `\` or a leading `.` is refused.
- `e2e::recorded()` is always false without the `e2e` feature.

## Side Effects

- The daemon's Unix socket (one connection, retried; `muster daemon --detach` when nothing answers).
- Tauri channels: the `StateMessage` stream, and raw frames for each attached terminal; window show/focus on `Reveal`.
- Files: `window.json` in the app data folder, written atomically (`.part`, then rename) before `keep` returns. An empty `settings.toml` is created when it is missing. `daemon.log` is checked for existence.
- Processes: `/usr/bin/open`, `/usr/bin/afplay`, `muster reload`. The native folder panel.
- `e2e` builds only: appends to `$MUSTER_E2E_RECORD/{opened,heard,revealed,edited}`; posts `NSEvent`s.

## Verification

- Run (in `apps/desktop/src-tauri`): `cargo test` (and `cargo test --features e2e` for the e2e-only code); `pnpm bindings` in `apps/desktop` if a type in `contract.rs` changed.
