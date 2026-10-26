# muster-state

## Overview

Library crate holding Muster's pure state machines: the session status
machine (hooks, terminal progress and exits to the status the person sees)
and the run machine (a ticket from pickup to a merged pull request). The
daemon feeds them signals, stores and carries out the effects they return,
and wakes them at their deadlines.

Dependencies: `muster-core`, `serde` (dev: `proptest`, `serde_json`).

## Interface

- [src](./src/README.md) — `session::{SessionMachine, Signal, HookEvent, Effect, Timing, Launch, Start, Carried, Origin}`, `run::{RunMachine, Limits, RunEvent, RunEffect, AgentOutcome, Verdict, Feedback}`.

## Invariants

- No IO and no clock of its own (`ARC-01`); depends only on `muster-core` and `serde`.
- No unsafe code (`ARC-12`).

## Side Effects

- None.

## Verification

- Run: `cargo check -p muster-state`, then `cargo nextest run -p muster-daemon -p muster-state`
- End-to-end: `cargo nextest run -p muster --test <area>` (`hooks`, `runs`, `reload`, `explain`, ...)
