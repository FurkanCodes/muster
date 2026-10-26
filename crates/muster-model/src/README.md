# muster-model/src

## Overview

A client's copy of the daemon's state, kept current from the first
`Snapshot` and the `Delta`s that follow. It decides nothing: it answers the
questions clients ask of it — what needs the person and in what order, how
the sessions list is laid out, how the agents stand. The terminal UI and the
desktop app both keep one so they answer alike (`ARC-24`), and both learn
from it which lines a space view lists (`ARC-31`).

## Interface

- `Model` (lib.rs) — the state as one client holds it; `replace(snapshot)`, `apply(delta)`, `loaded()`.
- `Model` lookups (lib.rs) — `projects`, `workspaces`, `sessions`, `project`, `workspace`, `session`, `workspaces_of`, `sessions_of`, `listed_of`, `run_of`, `checkout_of`, `branch`, `is_plain_folder`, `preferences`, `plugins`, `notifier`, `resources`, `usage`.
- `Model::needs_you` (lib.rs) — sessions waiting on the person: asking first, then unseen news; oldest first within each.
- `Model::working` / `counts` (lib.rs) — agents at work, longest first; `Counts { working, waiting, done }` for the top bar.
- `Model::rows` (lib.rs) — the sessions list: each space heading, its projects, worktrees, sessions, then free terminals.
- `Model::most_urgent` / `sessions_on` / `project_of` / `worktree_of` (lib.rs) — what a `Row` holds and how it stands.
- `Model::worktrees_for` / `agents_on` / `ticket_pull_request` (lib.rs) — a ticket's worktrees, agents and most telling pull request.
- `Model::agents_alive_in` / `second_agent_warning` / `under_its_chat` (lib.rs) — agents in a workspace and the warning before starting another (`PRODUCT.md` §6).
- `Row` (lib.rs) — one sessions-list line: `Project`, `Worktree`, `Session`, `Space` (`ARC-33`).
- `stands_beside` (lib.rs), re-exported `needs_attention` (from `muster_core::news`) — shell / own / plugin panel vs. an agent's chat; whether a session needs attention.
- `View` and `Model::rows_in` / `spaces` / `space` / `spaces_of` / `spaces_of_session` / `space_of` / `free_terminals` (spaces.rs) — what `View::All` or `View::Space(id)` lists.
- `fixtures` (fixtures.rs, `#[cfg(any(test, feature = "fixtures"))]`) — `project`, `worktree`, `session`, `free_terminal` builders for tests.

## Invariants

- No IO (`ARC-01`): no files, sockets, processes or wall clock; a client hands in what the daemon sent and reads answers. The *Needs you* order uses an internal counter (`clock`), not time.
- Until a snapshot arrives the model is empty and `loaded()` is `false` — "not loaded", not "no sessions".
- Projects are listed in the person's order (`Preferences::projects`), the rest oldest first; the sort is stable.
- `agents_alive_in` and `counts` never count a shell, the person's own terminal or a plugin's panel as an agent. `agents_on` (a ticket's agents) leaves out only plugin panels, so a shell in a ticket's worktree is listed among them.
- `rows_in(View::All)` lists items in no space last, so nothing drops out of sight between deltas; a space that no longer exists lists nothing.
- The `fixtures` module is compiled only for tests or with the `fixtures` feature.

## Side Effects

- None outside the `Model` value itself (in-memory mutation by `replace` / `apply`).

## Verification

- Run: `cargo check -p muster-model`, then `cargo nextest run -p muster-tui -p muster-model`
- End-to-end: `cargo nextest run -p muster --test 'tui*'`
