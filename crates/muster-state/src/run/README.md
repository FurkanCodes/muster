# muster-state/src/run

## Overview

The run machine: takes one ticket from pickup to a merged pull request, with
checks and an agent review before a person is asked (`PRODUCT.md` §5,
`ARCHITECTURE.md` §8). The module root `run.rs` (in the parent) re-exports
the public items. Unit tests live in `tests.rs` (not documented here).

## Interface

- `RunMachine` (machine.rs) — one run: `new(limits)`, `handed_off(limits)` (starts at the checks on a person's branch, `PRD-08`), `step(event, now) -> Vec<RunEffect>`, `next_deadline`.
- `RunMachine` persistence (machine.rs) — `save(now)` folds the current stretch of work into time spent; `restore(saved, now)` starts a new stretch (`PRD-09`); `resume_work()` re-asks the current stage's work under the current generation.
- `RunMachine` queries (machine.rs) — `stage`, `generation`, `attempt`, `pr`, `reason`, `limits`, `cost_micros`, `from_branch`.
- `Limits` (machine.rs) — `attempts` (default 3), `run_timeout` (default 90 min), `max_cost_micros` (default none) per budget (`PRD-12`).
- `RunEvent` (signal.rs) — `SlotGranted` (`PRD-05`), `ContainerReady`, `AgentFinished`, `ChecksFinished`, `PrOpened`, `ReviewSubmitted`, `InfraFailed`, `CostReported`, `PersonRequestedChanges`, `PrMerged`, `PrClosed`, `Cancel`, `Retry`, `TakeOver`, `HandBack`, `Tick`.
- `RunEffect` (signal.rs) — `Transition`, `StartContainer`, `StartBuilder`, `RunChecks`, `PublishBranch`, `StartReviewer`, `MarkReady`, `MarkDraft`, `StopWork { before }`, `StartInteractive`, `RemoveContainer`
- `AgentOutcome` / `Verdict` / `Feedback` (signal.rs) — how the builder's turn ended; the reviewer's approve / request-changes; why an attempt is made (failed checks, reviewer, person, person's notes).

## Invariants

- Pure (`ARC-01`): no IO, time comes in with every call; `Tick` at `next_deadline`.
- A failed check, a reviewer's and a person's request for changes each spend an attempt; past the budget the run needs the person, with the evidence (`PRD-02`, `PRD-12`).
- Time counts only while the run works on its own; a cost limit applies only where the harness reports cost; hitting either stops the work.
- A person's `Retry` or `HandBack` starts a fresh budget of attempts, time and cost.
- Every transition is returned first and stored, with its reason, before its work starts (`ARC-10`).
- The generation is bumped on every transition; reports carrying an older generation (work overtaken by a cancel, take-over or new attempt) change nothing.
- Every effect is safe to carry out again after a restart (starting a container looks for the run's first; publishing looks for an existing pull request).

## Side Effects

- None; `RunEffect`s are returned for the daemon to carry out (containers, git push, GitHub, agents).

## Verification

- Run: `cargo check -p muster-state`, then `cargo nextest run -p muster-daemon -p muster-state`
- End-to-end: `cargo nextest run -p muster --test runs`
