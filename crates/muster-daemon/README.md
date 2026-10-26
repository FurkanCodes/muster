# muster-daemon

## Overview

The Muster daemon crate: the long-running process that owns every session and
run and is the only process that talks to agents, Docker, git hosts and ticket
sources (`ARCHITECTURE.md` §4). It is a library; the `muster` CLI crate
(`crates/muster`) calls its `run` function to start the daemon and its other
public functions for `muster auth` and `muster plugin-run`. Clients (TUI,
desktop app, CLI) reach it over a Unix socket through `muster-client`; agents'
hooks reach it over a second socket.

Source and module contracts: [src/README.md](./src/README.md).

## Interface

- `run`, `DaemonConfig`, `DaemonError`, `Stop`, `HANDOFF_FORMAT` — start the
  daemon and run it until it stops or restarts in place.
- `plugin_run`, `PluginRunError`, `sign_in`, `SignIn`, `keep_secret`,
  `keep_jira_token`, `jira_site`, `bitbucket_account`, `plugin_account`,
  `LINEAR_KEY_ACCOUNT`, `KEYCHAIN_SERVICE`, `KEYCHAIN_ACCOUNT` — the CLI's
  plugin and sign-in entry points.
- `ServiceError`, `LogLimits` — re-exported errors and log bounds.
- `pub mod clock`, `pub mod config`, `pub mod store` — the clock trait,
  settings files, the SQLite store.

## Dependencies

- Workspace crates: `muster-core` (ids, entities, protocol, settings
  readers), `muster-state` (the session and run state machines),
  `muster-client` (the framed transport).
- Runtime: `tokio`, `axum` (hook receiver), `portable-pty` and `vt100`
  (terminals), `rusqlite` (store), `reqwest` (HTTP APIs and MCP), `nix` /
  `rustix` (process groups, descriptors), `sysinfo` (resource use),
  `rmp-serde` / `serde_json` (framing), `sha2` (plugin manifest hashes).
- Dev: `proptest`, `tempfile`, `tokio` with `test-util` (paused time).

## Invariants

- Docker, GitHub, ticket sources, harnesses, PTYs and the clock are reached
  only through traits (`ARC-03`).
- The Supervisor alone owns the daemon's state (`ARC-02`), and writes the
  store before it changes state (`ARC-10`).
- No source file over 1,000 lines, no struct over 40 fields (`ARC-08`).
- No real credential in code, fixtures, logs or test output (`PRD-06`).

## Side Effects

- Unix sockets, a lock file and a log in the private runtime directory.
- SQLite database, repository mirrors, timelines and plugins in the data
  directory.
- Child processes: PTYs, agents, git, `gh`, `glab`, `docker`, plugins.
- Network calls to MCP servers and tracker / git host APIs; macOS Keychain.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon`
- End to end: `cargo nextest run -p muster --test <area>` (`hooks`, `runs`,
  `worktrees`, `sessions`, …)
- Real Docker (before a release only): `MUSTER_DOCKER_TESTS=1 cargo test -p muster-daemon`
