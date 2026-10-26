# timeline

## Overview

The submodules of `src/timeline.rs`, workspaces' timelines (`PRODUCT.md` §6,
`PRD-14`): the files of a workspace as each agent's turn begins and ends, kept
so the person can see what each turn changed and put the files back as they
were before it.

Each workspace's timeline is a bare git repository of Muster's own in the data
directory, borrowing the workspace repository's objects through git
alternates. An entry is a commit under `refs/timeline/`; a turn under way has
its start under `refs/open/<session>`. This directory adds undoing an entry
and moving a workspace's branch onto another commit.

## Interface

From the module root `src/timeline.rs`:

- `Store` (timeline.rs) — one workspace's timeline: `open`, `open_with_index`,
  `snapshot`, `commit`, `timeline`, `newest`, `diff`, `began`, `ended`,
  `inside`, `inside_allowing`.
- `Snapshot` (timeline.rs) — the workspace's files at a moment.
- `TimelineError` (timeline.rs) — what went wrong.
- `dir(data_dir, workspace)`, `remove(dir)` (timeline.rs) — where a timeline
  lives; removing it with its workspace.

In this directory:

- `Store::plan(number)` (undo.rs) — what undoing an entry would do
  (`UndoPlan`).
- `Store::undo(...)` (undo.rs) — puts the files (and the branch, where it can
  go) back as they were before the entry, keeping the present as an entry
  first.
- `MovePlan`, `MoveDone` (moving.rs) — a branch move and its result.
- `Store::move_onto(...)` (moving.rs) — replays the branch's own commits onto
  the newest of its stack parent or base with `git rebase --onto`
  (`muster_core::stacks`).

## Invariants

- Taking snapshots only reads the person's index, refs and objects, through an
  index of the store's own. Only undo and `move_onto` (a `git rebase --onto`
  in the workspace) change the workspace and its branch, and only once the
  person says yes.
- Each entry's single parent is a parentless commit of the files as the turn
  found them, so letting an entry go takes nothing else with it.
- The store is the whole record: nothing of it is in the database, and it goes
  with its workspace (`ARC-14`).
- Undo and move keep what is there now as an entry of its own first, so they
  can be undone.
- A move that would conflict is aborted and changes nothing.

## Side Effects

- Runs `git` against the timeline's bare repository and the workspace.
- Writes and removes the timeline directory under the data directory.
- Undo and move rewrite the workspace's files and may move its branch.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon timeline`
- End to end: `cargo nextest run -p muster --test timeline --test stacks --test tui_timeline`
