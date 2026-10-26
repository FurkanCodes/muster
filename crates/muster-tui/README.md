# muster-tui (crate)

## Overview

The terminal UI library for Muster, linked by the `muster` binary, which runs
it when invoked with no subcommand. It shows what the daemon reports and
sends the person's commands back, and holds no business logic of its own. The
code and its contract are in [`./src/`](./src/README.md).

## Interface

- Library API (from `src/lib.rs`): `run`, `Options`, `Connector`, `TuiError`,
  `START_WAIT`, `START_AGAIN_AFTER`, and `keys::{encode_key, encode_paste}`.
  See [`./src/README.md`](./src/README.md).
- `Cargo.toml`. Dependencies: `ratatui`, `crossterm`, `tui-term`, `vt100`,
  `tokio`, `tokio-util`, `futures-util`, `muster-client`, `muster-core`,
  `muster-model`, `nix`, `base64`, `unicode-width`, `thiserror`.
  Dev-dependencies: `muster-model` with `fixtures`, and `tempfile`.
- `assets/logos/` — agents' logos (SVG sources, 64-pixel PNGs, raw `.rgba`)
  compiled into the view with `include_bytes!`. Their sources and licences are
  in [`./assets/logos/README.md`](./assets/logos/README.md).

## Invariants

- The UI depends on `muster-model` for its view model and on `muster-client`
  for the wire. It never links `muster-daemon` or `muster-state`.
- `App` and `view` do no IO and never read the clock; the runtime does
  (`ARC-03`). `#![forbid(unsafe_code)]` (`ARC-12`).
- No source file over 1,000 lines and no struct over 40 fields (`ARC-08`).
- Workspace lints apply (`[lints] workspace = true`).

## Side Effects

- Only when `run` is called; see [`./src/README.md`](./src/README.md)
  (terminal modes, escape sequences, clipboard programs, the daemon socket).

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui -p muster-model`
- End to end: `cargo nextest run -p muster --test 'tui*'`
