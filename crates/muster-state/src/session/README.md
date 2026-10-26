# muster-state/src/session

## Overview

The session status machine (`ARCHITECTURE.md` §5): turns hook events, the
terminal's progress sequence (OSC 9;4), the harness's display and process
exits into the status and "unseen" mark the person sees. The module root
`session.rs` (in the parent) re-exports the public items; this directory
holds its parts. Unit tests live in `tests.rs` and `tests/` (not documented
here).

## Interface

- `SessionMachine` (machine.rs) — one session's status: `new(launch, timing, now)`, `step(signal, now) -> Vec<Effect>`, `status`, `unseen`, `agent_session`, `subagent_at_work`, `next_deadline`; plus `carry` / `adopt` / `explain` / `heeds` from [machine](./machine/README.md).
- `Timing` (machine.rs) — time windows: `drain_grace` 180 s, `subagent_quiet` 30 min, `heal_window` 30 s, `echo_window` 5 s, `start_grace` 10 s (defaults).
- `Launch` / `Start` (machine.rs) — how a session was launched (agent session id, `Idle` / `WithPrompt` / `Resumed`, unseen, `reports_start`).
- `Carried` (machine/carry.rs) — re-exported; see [machine](./machine/README.md).
- `Signal` (signal.rs) — `Hook { agent_session, event }`, `Progress { busy }`, `Waiting { waiting }`, `Exited { success, requested }`, `Watched { watching }`, `Tick`.
- `HookEvent` (signal.rs) — harness-neutral hooks: `SessionStarted`, `PromptSubmitted`, `TurnEnded`, `TurnInterrupted`, `PermissionRequested`, `QuestionAsked`, `ToolStarted`, `ToolFinished`, `Idle`, `SubagentStarted`, `SubagentStopped`.
- `Effect` (signal.rs) — `Status { status, unseen }` (store and tell clients), `SaveResumeId(id)`.
- `Origin` (signal.rs) — re-export of `muster_core::session::Origin` (main agent or subagent).
- `Signal::describe` / `HookEvent::describe` (describe.rs) — human-readable signal kinds for `muster explain`.

## Invariants

- Pure (`ARC-01`): no IO, time comes in with every call; the daemon calls `step(Signal::Tick, ..)` at `next_deadline`.
- The machine never sees harness-specific names; drivers in the daemon translate hooks to `HookEvent`.
- Hooks from a harness session other than the one this process runs are ignored, except the start announcement after `/clear`.
- The main agent ending its turn does not finish the session while subagents work; `drain_grace` follows the last one.
- Several prompts can be open at once; the newest is shown. A permission prompt closes on a tool event from whoever raised it; a question only on its own completion; `TurnInterrupted` closes every prompt.
- A deferred permission/question notice within `echo_window` of an answer is an echo and opens nothing.
- A progress idle with no busy edge before it is the startup state.
- While the terminal is watched nothing becomes unseen.
- The resume id is saved only once a turn has run in the harness session, so it never points at an empty transcript.
- `describe` names only a signal's kind and origin, never its payload (`PRD-06`).

## Side Effects

- None; effects are returned as `Effect` values for the daemon to carry out.

## Verification

- Run: `cargo check -p muster-state`, then `cargo nextest run -p muster-daemon -p muster-state`
- End-to-end: `cargo nextest run -p muster --test hooks`
