# plugins/hooks

## Overview

The submodules of `src/plugins/hooks.rs`: one run of one plugin hook. A run
checks the plugin's manifest against the hash the person approved, fills in
the hook's command from its event, starts the program in the plugin's folder
in a process group of its own, writes the event to its stdin, reads stdout and
stderr together under a cap, and stops the whole group at its deadline
(`ARC-13`, `ARC-23`). The outcome goes to the plugin's log and to whoever
waits on it (`muster plugin hook`).

The queueing, concurrency limits and shutdown of hook runs live in the parent
module, `src/plugins/hooks.rs` (`Hooks`, `Job`, `Limits`); see
[../README.md](../README.md).

## Interface

All three are `pub(super)`, for `plugins/hooks.rs` only.

- `Given<'a>` (run.rs) — what a run is handed besides its `Job`: `secrets`,
  `limits`, the plugin's `log`, the `cut` flag and a `started` callback given
  the process group. The environment basics are read at spawn time
  (`trackers::plugin::basics`).
- `run(job, given)` (run.rs) — one hook run, start to finish; reports a
  `HookRun` on the job's `done` channel if one is set.
- `entry(hook, event, outcome, millis, output)` (run.rs) — formats the line a
  run (or a run refused) appends to its plugin's log.

## Invariants

- A manifest whose SHA-256 no longer matches the approved one runs nothing
  (`PRD-15`).
- The program runs in its own process group; at the deadline the group is told
  to stop, then killed after `Limits::stop_grace`, so nothing it started is left
  running (`ARC-13`).
- The environment is the basics sessions get plus only the variables and
  secrets the manifest names (`ARC-23`).
- Output (stdout and stderr together) is capped at `Limits::output` (16 KiB by
  default) (`ARC-14`).

## Side Effects

- Spawns a child process (the hook's program) in the plugin's folder.
- Writes the event JSON to the child's stdin; reads its pipes.
- Sends signals to the child's process group.
- Appends to the plugin's in-memory log (`Arc<Mutex<Log>>`).

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon plugins::hooks`
- End to end: `cargo nextest run -p muster --test hooks --test plugins`
