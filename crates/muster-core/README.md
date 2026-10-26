# muster-core

## Overview

The shared, IO-free base crate of the Muster workspace. Every other Muster
crate depends on it for ids, entities, states, the daemon and client wire
protocol, framing, and the strict readers of settings and plugin manifests.
The source and its contracts are in [`./src/README.md`](./src/README.md).

The crate contains:

- `Cargo.toml`. Its settings come from the workspace (version, edition,
  license, lints).
  - Dependencies: `serde`, `serde_json`, `rmp-serde`, `serde_bytes`, `toml`,
    `toml_edit`, `thiserror` and `ulid`. `ts-rs` is optional.
  - Feature `ts` turns on `ts-rs` to export TypeScript bindings of the
    protocol types for the desktop app (`ARC-24`). It is off for every other
    build.
  - Dev dependency: `proptest`.
- `src/`: the library. See [`./src/README.md`](./src/README.md).
- `tests/`: integration tests and the samples they read. No README.
  - `frozen/v28`, `frozen/v30` … `frozen/v58`: the frozen handshake frames of
    each protocol version (none was kept for v29), read by
    `frozen_protocol.rs` (`ARC-09`).
  - `protocol_round_trip.rs`: round-trips the messages built by its
    `*_samples` and `variants` submodules.
  - `frame_props.rs`: proptest properties of the framing.

## Interface

- Library crate `muster_core`. See [`./src/README.md`](./src/README.md) for
  its modules and root re-exports.
- Cargo feature `ts`: TypeScript bindings via `ts-rs`.

## Invariants

- No IO (`ARC-01`). Outside tests, nothing touches the filesystem, network,
  processes or environment. The one thing taken from the system is a new id:
  each id type's `new()` calls `Ulid::generate()`, which reads the clock and
  randomness (`src/ids.rs`).
- `#![forbid(unsafe_code)]` (`ARC-12`).
- The protocol versioning rules (`ARC-09`). The current protocol version is 58,
  and frozen samples are added, never edited, when it is raised.
- No real credentials in code, fixtures or samples (`PRD-06`).

## Side Effects

- None at run time. Test runs with `--features ts` may write bindings into
  `TS_RS_EXPORT_DIR`.

## Verification

- Run: `cargo check -p muster-core`, then `cargo nextest run -p muster-core`.
- After a protocol version change: `cargo nextest run --workspace` (`ARC-09`)
  and `pnpm bindings` in `apps/desktop`.
