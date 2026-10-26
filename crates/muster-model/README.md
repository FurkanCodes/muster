# muster-model

## Overview

"A client's copy of Muster's state, and what it says: what needs the person,
in what order." A pure library shared by the TUI and the desktop app so both
order, list and count sessions the same way (`ARC-24`, `ARC-31`).

Dependencies: `muster-core` only. Feature `fixtures` exposes entity builders
for the clients' own tests.

## Interface

- [src](./src/README.md) — `Model`, `Row`, `View`, `Counts`, `stands_beside`, `needs_attention`, `fixtures`.

## Invariants

- No IO (`ARC-01`): depends only on `muster-core`; reached by clients that feed it daemon frames.

## Side Effects

- None.

## Verification

- Run: `cargo check -p muster-model`, then `cargo nextest run -p muster-tui -p muster-model`
- End-to-end: `cargo nextest run -p muster --test 'tui*'`
