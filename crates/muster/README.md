# muster (crate)

## Overview

The `muster` binary crate: "a terminal cockpit for coding agents: assign them
to your tickets, each in its own git worktree". It is the one crate `dist`
builds and ships (`[package.metadata.dist] dist = true`); the workspace is not
published to crates.io.

The binary puts the other crates together. It runs the daemon
(`muster-daemon`), opens the terminal UI (`muster-tui`), runs the macOS notch
(`muster-notch`), and talks to a running daemon (`muster-client`,
`muster-core`). The CLI's code and contract are in [`./src/`](./src/README.md).

## Interface

- The `muster` executable. Its subcommands are described in
  [`./src/README.md`](./src/README.md): the UI (no subcommand), `daemon`,
  `reload`, `upgrade`, `status`, `project`, `check`/`review`/`pr`, `run`,
  `plugin`, `auth`, `notch`, `machine`/`ssh`/`--on`, and the shell verbs
  (`list`, `send`, `wait`, `attach`, `workspace`, `space`, `term`, `ticket`,
  `init`, `muster <agent>`).
- `Cargo.toml`. Dependencies: `clap`, `crossterm`, `muster-client`,
  `muster-core`, `muster-daemon`, `muster-notch`, `muster-tui`, `nix`,
  `reqwest`, `serde`/`serde_json`, `sha2`, `tempfile`, `thiserror`, `tokio`,
  `tracing`, `tracing-subscriber`. Dev-dependencies for the end-to-end suite:
  `rusqlite`, `portable-pty`, `tempfile`, `toml`, `vt100`.
- `tests/` — the end-to-end suite. Each file starts the real daemon binary,
  with fakes for external systems (`lifecycle`, `detach`, `slow_client`,
  `hooks`, `plugins`, `runs`, `cli_ssh`, `notch`, `tui*`, …). It has no
  README of its own.

## Invariants

- Workspace lints apply (`[lints] workspace = true`); `src` forbids unsafe
  code (`ARC-12`).
- The binary may do IO. The rules that keep IO out of `muster-core`,
  `muster-state` and `muster-model` (`ARC-01`) do not apply here, but external
  systems are still reached through their traits (`ARC-03`).
- No real credential in code, fixtures, logs or test output (`PRD-06`).

## Side Effects

- Building it builds every crate it links. Running it is covered in
  [`./src/README.md`](./src/README.md).

## Verification

- Run: `cargo check -p muster`, then the end-to-end files for the area
  touched: `cargo nextest run -p muster --test <area>` (several `--test`
  flags or a glob, e.g. `--test 'tui*'`). Run the whole suite only for a
  protocol, dependency or toolchain change, or for a release.
