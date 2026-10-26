# supervisor

## Overview

The submodules of `src/supervisor.rs`: the Supervisor, the one task that owns
the daemon's state (`ARC-02`). Every change goes through it: it writes the
change to the store first (`ARC-10`), then updates its copy of the state, then
broadcasts the delta. Because it alone takes snapshots and hands out
subscriptions, a subscriber sees exactly the deltas after its snapshot, with no
gap and no repeat.

Terminal traffic does not pass through it: a connection asks once for a
session's `PtyHandle` and then talks to the terminal's actor directly. Only
resizes come here, since the Supervisor decides which client's size a terminal
takes (`sizing.rs`). Slow work (git, trackers, git hosts, Docker) runs off the
loop under deadlines and reports back (`ARC-13`).

## Interface

From the module root `src/supervisor.rs`:

- `spawn(settings)` (supervisor.rs) — starts the Supervisor task; returns a
  `SupervisorHandle`.
- `Settings` (supervisor.rs) — what it is started with: the `StoreHandle`, a
  `Clock`, the worktrees, data and config directories, the hook socket and
  queue, and the outside systems behind their traits (`RunWork`, `Trackers`,
  `Hosts`, `Credentials`, `Secrets`; `ARC-03`), plus sessions adopted from a
  restart in place.

In this directory:

- `SupervisorHandle` (handle.rs) — how the rest of the daemon reaches it:
  `command(Command) -> Result<Reply, ErrorBody>`, `subscribe`, `terminal`,
  `resize`, `add_client`, `looking`, `colours`, `hold_keys`, `ended`,
  `record_spends`, `spend_targets`, `record_interrupted`, `record_stopped`,
  `record_resources`, `check_pull_requests`, and `hand_over` / `adopt` for the
  restart in place (`stop` is test-only).
- `Subscription` (handle.rs, `pub(super)`) — `(Snapshot,
  broadcast::Receiver<Delta>)`: what `subscribe` returns, a state snapshot
  plus the deltas after it.
- `TerminalLink`, `WatchGuard` (re-exported by `supervisor.rs`) — what
  `server.rs` holds for a session's terminal and its watching.
- `ClientGuard` (clients.rs) — a connection counted among windows and terminal
  clients; dropped, it is forgotten.
- `KeysGuard` (terminals.rs) — counts one client with the person's keys in a
  session's terminal until dropped.

Files by responsibility:

| File | Role |
|---|---|
| `parts.rs` | The state in parts: folders, outside systems (`ARC-03`), clients |
| `handle.rs`, `clients.rs`, `lookup.rs` | Requests on the queue; connections; finding what a command names |
| `sessions.rs`, `status.rs`, `terminals.rs`, `explain.rs` | Starting, stopping, restarting sessions; status machines fed by hooks; which terminal each has; why a status |
| `recovery.rs`, `reload.rs` | Sessions brought back after a restart (`PRD-09`); restart in place (`handoff`) |
| `sizing.rs`, `looking.rs`, `fronts.rs` | Whose size a terminal takes; who looks at windows (notch, `ARC-22`); the foreground program (`ARC-28`) |
| `projects.rs`, `cloning.rs`, `spaces.rs`, `free.rs` | Projects and checkouts; cloning from GitHub; spaces (`ARC-33`); free terminals |
| `worktrees.rs`, `worktree_planning.rs`, `outside_worktrees.rs`, `shared_builds.rs` | Git worktrees: create, remove, plan, adopt ones made outside, shared build folders |
| `changes.rs`, `clashes.rs`, `stacks.rs`, `tidy.rs`, `timeline.rs` | What work changed; where workspaces meet; stacked branches; tidying commits; turn timelines (`PRD-14`) |
| `runs.rs`, `containers.rs`, `by_hand.rs`, `prepared.rs` | Runs' machines and work (`ARC-10`); sessions in run containers (`PRD-08`); stages by hand; sessions that ask git first |
| `issues.rs`, `pulls.rs` | Tickets from trackers; pull request state from git hosts |
| `plugins.rs`, `panels.rs`, `events.rs` | Plugin commands and registry; plugin panels (`ARC-23`); events to hooks (`PRD-15`) |
| `names.rs`, `naming.rs` | Chat names from the first prompt, and from an agent |
| `settings.rs`, `own_settings.rs`, `spend.rs` | Settings screen (`ARCHITECTURE.md` §12); Muster's own settings; agents' spend (`PRD-12`) |

## Invariants

- The Supervisor alone owns and changes the daemon's state (`ARC-02`).
- A change is written to the store before the in-memory state changes and
  before its delta is broadcast (`ARC-10`); a run's transition is stored before
  its work starts.
- Subscribers see exactly the deltas after their snapshot: no gap, no repeat.
- Keystrokes and terminal output never pass through the Supervisor.
- Git, trackers, hosts and Docker are asked off the loop, under deadlines
  (`ARC-13`), and only through their traits (`ARC-03`).
- Every project is in exactly one space, and a free terminal in one or none
  (`ARC-33`); only `spaces.rs` keeps it so.
- Muster never types into a person's shell; tidy never runs while an agent
  works in the workspace.
- Clashes are kept on each workspace and never stored.

## Side Effects

- Writes the SQLite store through `StoreHandle`.
- Broadcasts state deltas to subscribed clients.
- Spawns sessions (PTYs), git, Docker and plugin processes through the crate's
  modules.
- Creates and removes git worktrees and folders under the worktrees and data
  directories.
- Raises events to plugin hooks.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon supervisor`
- End to end, by area: `cargo nextest run -p muster --test sessions --test worktrees --test runs --test spaces --test reload` (add `hooks`, `panels`, `stacks`, `tidy`, `timeline`, `projects`, … for the file touched)
