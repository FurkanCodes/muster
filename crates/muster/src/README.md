# muster/src (the `muster` CLI)

## Overview

The `muster` binary, a clap CLI and the one crate that ships. With no
subcommand it opens the terminal UI (`muster_tui::run`), starting a local
daemon if none answers.

`main()` handles three cases before it builds any tokio runtime:

- `daemon --from-job FILE` (the launchd job) goes to `launchd::run_job`.
- `plugin-run ID` serves a built-in ticket plugin on stdio.
- `notch` runs on the main thread, as AppKit needs.

Everything else resolves `Paths` and applies `--on MACHINE` /
`MUSTER_MACHINE`, sending the command to another machine over the person's
ssh. It then runs the subcommand on a multi-thread runtime. Most handlers talk
to the daemon over its Unix socket through `muster_client::Connection`
(`commands::connect`).

## Interface

- `main`, `Cli` (main.rs) — the clap root with the global `--on MACHINE`.
- `Commands` (main.rs) — `Daemon` (run, `stop`, `--detach`; hidden
  `--background`, `--from-job`, `--adopt`, `--can-adopt`), `Reload`, `Upgrade`,
  `Status`, `Project add|remove|config`, `Check`, `Review`, `Pr`, `Run`
  (hidden), `Plugin`, `Auth`, `Notch`, `Verb` (flattened shell verbs), `Agent`
  (`muster <agent> [PROMPT]`), `PluginRun` (hidden).
- `on_another_machine`, `ui`, `start_in_background`, `open_app`, `fail`
  (main.rs) — which commands may take `--on`; open the UI; detach the daemon;
  open Muster.app; print `muster: <msg>` and exit 1.
- `commands::connect`, `commands::status` (commands.rs) — connect locally or
  over ssh; `muster status [--json]`.
- The other handlers `main.rs` dispatches to (commands.rs, `pub(crate)`):
  `add_project`, `remove_project`, `show_config`, `stop_daemon`,
  `list_issues`, `check`, `review`, `open_pull_request`, `store_claude_token`,
  and the guard `no_agent_starts_one` (refuses inside an agent's session).
- `daemon_run::daemon` (daemon_run.rs) — tracing (`MUSTER_LOG`), panic hook,
  `DaemonConfig` env overrides and signals, around `muster_daemon::run`.
- `daemon_start::{connect, stream, start}` (daemon_start.rs) — connect,
  starting `muster daemon --detach` and polling if none answers.
- `launchd::{wanted, Job, start, run_job}` (launchd.rs) — the one-shot launchd
  job in `gui/<uid>` that starts the daemon on macOS.
- `reload::reload_daemon` (reload.rs) — moves the daemon onto the installed
  binary in place (SIGUSR1).
- `auth::{bitbucket, jira, linear, plugin}` (auth.rs) — read a token without
  echo, or sign in through the browser, and keep it in the Keychain.
- `mac_app::ensure` (mac_app.rs) — builds the self-signed `Muster.app` in the
  data dir so macOS shows notifications.
- `json::print` (json.rs), `explain::explain_session` (explain.rs),
  `output::Position` (output.rs), `person::{agrees, open_in_browser}`
  (person.rs) — terminal-safe JSON (`format` 2); `muster explain`; stream
  positions that count missed bytes; y/N prompts and the browser.
- `runs.rs` — `muster run` (autopilot): `start`, `list`, `show`, `cancel`,
  `retry`, `request_changes`, `take_over`, `hand_back`, `find_run`. Its tests
  live in `runs/tests.rs`.
- `plugins.rs` — `muster plugin` (`PluginCommand`, `run`); output formats in
  [`./plugins/`](./plugins/README.md).
- `ssh.rs` — `muster machine`, `--on`, `muster ssh` (`ARC-27`); carrier and
  options in [`./ssh/`](./ssh/README.md).
- `upgrade.rs` — `muster upgrade` (`run`, `release_binary`, `ARC-21`); the Mac
  app in [`./upgrade/`](./upgrade/README.md).
- `verbs.rs` — `Verb`, the shell verbs that drive agents; in
  [`./verbs/`](./verbs/README.md).

## Invariants

- `#![forbid(unsafe_code)]` (`ARC-12`).
- Commands read time only through `instant_now()`, and `launchctl` sits behind
  the `Launchd` trait (`ARC-03`). Every sleep or poll is deliberate and has a
  deadline (`ARC-30`).
- The daemon is never the UI's child, so closing the UI or its terminal leaves
  sessions running (`PRD-09`). On macOS that is done with a one-shot launchd
  job, which is not used over SSH or off macOS.
- Tokens never appear on a command line or on screen. They are read with echo
  off or from piped stdin and kept in the macOS Keychain (`PRD-06`). JSON
  prints no secrets.
- No raw control characters reach the terminal: output goes through
  `terminal_safe`, and `fail()` cleans its message (`PRD-07`).
- launchd env and plist files are owner-only (0600 in a 0700 runtime dir) and
  removed whatever happens. A job that hangs is booted out.
- `reload` signals only daemons from version 0.2.0 on; `--can-adopt` succeeds
  only for `muster_daemon::HANDOFF_FORMAT`.
- Plugin install, link, update and enable ask on a terminal, are refused
  without one unless `--yes`, and are refused inside a Muster session
  (`PRD-01`, `PRD-15`). Run-changing `muster run` verbs are refused inside an
  agent's session.
- An upgrade always checks SHA-256 before it swaps by rename, and the
  attestation too where the GitHub CLI can, unless `MUSTER_SKIP_ATTESTATION`
  is set or the release is a stand-in; where it cannot, the checksum alone is
  taken and said, unless `MUSTER_REQUIRE_ATTESTATION` refuses it (`ARC-21`).
- No source file over 1,000 lines (`ARC-08`).

## Side Effects

- Starts the daemon in one of two ways:
  - `muster daemon --detach`, with stderr to `<runtime_dir>/daemon.log`;
  - on macOS, through `launchctl bootstrap` of a temporary plist; otherwise
    `--background` + `setsid`.
- Unix-socket connections to the daemon, or an `ssh` child with `--on`.
- Signals: SIGUSR1 to the daemon for `reload`, SIGTERM for
  `muster daemon stop`. The daemon run here handles
  SIGINT, SIGTERM, SIGHUP and SIGUSR1.
- Network: release downloads (`upgrade`, reqwest). Sign-in and plugin traffic
  go through `muster_daemon`.
- Keychain writes; termios echo off while reading secrets.
- Spawns `open` / `xdg-open`, `/usr/bin/open` for Muster.app, `gh`, `tar`,
  `ssh`, and `muster notch` (stderr to `notch.log`).
- Writes `Muster.app` in the data dir, `daemon.log` and `notch.log`.
- stdout/stderr output. Reads `MUSTER_*`, `SSH_CONNECTION` and `SSH_TTY`.

## Verification

- Run: `cargo check -p muster`, then the end-to-end files for the commands
  touched: `cargo nextest run -p muster --test <area>`.
