# muster-state/src/session/machine

## Overview

Submodules of `session/machine.rs` that extend `SessionMachine` with two
extra `impl` blocks: carrying a machine across an in-place daemon restart
(`muster reload`), and explaining what a machine holds for `muster explain`.
Both are pure: time is always handed in.

## Interface

- `Carried` (carry.rs) — a serde-serializable `SessionMachine` snapshot: status, unseen, agent session, resume id, open prompts, last answered prompt, hold for subagents, start deadline — every moment stored as "how long before the handoff".
- `SessionMachine::carry(now)` (carry.rs) — the machine as `Carried` at `now`.
- `SessionMachine::adopt(carried, timing, now)` (carry.rs) — rebuilds the machine at the new daemon's `now`, unwatched.
- `SessionMachine::explain(now, wall)` (explain.rs) — a `muster_core::explain::MachineView` (open waits, hold, next deadline) with each moment on the wall clock.
- `SessionMachine::heeds(signal)` (explain.rs) — the one filter `step` applies: hooks from another harness session are ignored except its start announcement; after exit only `Watched` counts.

## Invariants

- No IO and no clock of its own (`ARC-01`): `Instant` and `Timestamp` are parameters.
- An `Instant` never crosses the process boundary; `Carried` holds durations (start deadline in signed milliseconds, negative once passed), and a moment before the clock began clamps to `now`.
- Watching is not carried: after `adopt` the machine is unwatched until clients say otherwise.
- Open prompts, working subagents, holds and the start grace continue across `carry` → `adopt` as if no restart happened.

## Side Effects

- None (pure value transformations).

## Verification

- Run: `cargo check -p muster-state`, then `cargo nextest run -p muster-daemon -p muster-state`
- End-to-end: `cargo nextest run -p muster --test reload --test explain`
