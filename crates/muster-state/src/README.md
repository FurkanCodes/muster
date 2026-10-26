# muster-state/src

## Overview

Muster's pure state machines. Each one takes a signal (or event) and the
current time and returns the effects for the daemon to carry out. Two
machines: the session status machine and the run (ticket pipeline) machine.
`lib.rs` declares the modules; `session.rs` and `run.rs` are module roots
that document each machine's rules and re-export its items from the
directories below.

## Interface

- `lib.rs` — `pub mod run; pub mod session;`, `#![forbid(unsafe_code)]`.
- `session.rs` — re-exports `SessionMachine`, `Carried`, `Launch`, `Start`, `Timing`, `Signal`, `HookEvent`, `Effect`, `Origin`; contracts in [session](./session/README.md).
- `run.rs` — re-exports `RunMachine`, `Limits`, `RunEvent`, `RunEffect`, `AgentOutcome`, `Feedback`, `Verdict`; contracts in [run](./run/README.md).

## Invariants

- No IO and no clock of its own (`ARC-01`): `Instant` / `Timestamp` are always parameters, effects are returned values. No `std::fs`, `std::net`, `std::process`, `SystemTime` or `Instant::now` outside tests.
- No unsafe code (`#![forbid(unsafe_code)]`, `ARC-12`).
- Each machine is driven only through `step(.., now)` plus `Tick` at its `next_deadline`; the daemon owns timers.
- Run transitions are stored before their work starts (`ARC-10`).

## Side Effects

- None.

## Verification

- Run: `cargo check -p muster-state`, then `cargo nextest run -p muster-daemon -p muster-state`
- End-to-end: `cargo nextest run -p muster --test hooks --test runs`
