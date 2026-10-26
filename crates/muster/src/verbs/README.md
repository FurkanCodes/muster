# verbs (submodules of `verbs.rs`)

## Overview

The shell-facing verbs of the `muster` binary (`PRODUCT.md` §6b): what a person
or a script, an agent among them, types to drive the coding agents and
terminals the daemon runs. With them you can start an agent in a new worktree,
list what exists, send a prompt or keys, wait for an outcome, read a screen,
show a session in the current tab, and manage workspaces, spaces, terminals
and tickets.

Most verbs follow the same steps: connect to the daemon socket
(`commands::connect(&paths.socket())`), subscribe for a `Snapshot`, work out
which session or workspace is meant (`targets.rs`), and then send
`muster_core::protocol::Command`s through a `muster_client::Connection`. The
`Verb` clap enum and its dispatch live in the parent
[`../verbs.rs`](../verbs.rs).

## Interface

- `Args` (start.rs) — `muster <agent> [PROMPT]` (claude, codex, gemini,
  opencode, copilot or a configured `[agents.<name>]`); `--here`, `--in WS`,
  `-p/--project`, `--branch`; `-` reads the prompt from stdin.
- `Listing` (list.rs) — `muster list [workspaces|agents|terminals|sessions|projects|tickets|machines|spaces]`,
  scoped to the current workspace or project, or `--all` / `--project` /
  `--space`; `--json`.
- `send`, `prompt` (send.rs) — paste words, then press Enter; `MODES_WAIT`,
  `modes()` shared with control.rs.
- `keys`, `drivable` (control.rs) — `muster keys <s> KEY…` encoded per the
  session's terminal modes; also `muster wait … --for TEXT` and `muster move`.
- `Until`, `ANY_STOP`, `Rest::code` (wait.rs) — `muster wait` until Finished,
  NeedsYou or Exited; exit codes 0 / 3 / 4, 124 when time runs out, 5 when an
  agent stalls.
- `run` (logs.rs) — `muster logs`: the screen, the last reply, or follow mode.
- Attach logic, `PUT_BACK` (tab.rs) — `muster attach` / `muster enter`: shows a
  session in this tab; Ctrl-Q goes back.
- `WorkspaceCommand` (workspaces.rs) — `muster workspace add|rm` (alias `ws`;
  new branch `muster/NAME`).
- `SpaceCommand` (spaces.rs) — `muster space add|rename|mv|rm`.
- `TermCommand` (terminals.rs) — `muster term new [NAME] [--in WS | --free [--space S]] [--bg]`,
  its only subcommand. `terminals::rename` and `terminals::close` back the
  top-level verbs `muster rename` and `muster close`.
- `ticket_key`, `run` (tickets.rs) — `muster ticket KEY [open|close|assign [WS]|unassign|<agent>]`;
  takes `42`, `#42` or `APP-7`.
- `Shell`, `run` (init.rs) — `muster init zsh|bash|fish`: prints a wrapper
  function per agent name.
- `Asked`, `run` (exec.rs) — hidden `muster exec AGENT [WORDS…]`: gets the
  command line from the daemon (`Command::ExecAgent`) and runs the agent in the
  foreground.
- `find_session`, `find_session_in`, `current_workspace`, `short` (targets.rs)
  — resolve a session by number, `WS/N`, full id, 4+ letter id suffix, name or
  branch; the short id is the last 6 letters of its ULID.

## Invariants

- An agent may drive sessions as the person could, but never answers what
  another agent asks the person: `drivable` refuses a NeedsYou session when the
  caller runs inside an agent's session (`PRD-01`).
- `muster logs` never watches a session, so its news stays news for the
  person's Needs-you (`PRD-01`).
- The ticket verbs that change something (`close`, `assign`, `unassign`, and
  starting an agent on a ticket) refuse inside an agent's session; which
  tickets get worked on is the person's choice (`PRD-01`, `PRD-07`). Reading
  a ticket and `open` work anywhere.
- `muster init` writes no file of the person's; it only prints (`PRD-13`).
- An agent typed by name in a Muster terminal runs through `muster exec` with
  Muster's hooks. Outside one, with a flag first, or with piped input, the
  agent's own program runs unchanged (`ARC-28`). If the daemon cannot give the
  command line, `exec` falls back to the agent's own program.
- Each project is in exactly one space, and each terminal outside a project in
  one or none; `space rm` only works on an empty space (`ARC-33`).
- Exited sessions are refused (suggesting `muster restart`); every key name is
  checked before any key is pressed.
- A target that matches nothing or several sessions is refused with the
  reason. Workspace sessions are numbered oldest first.
- `workspace rm` asks before it stops sessions (unless `-y`), refuses
  uncommitted changes (unless `--force`) and keeps the branch.
- Going back from `attach` (Ctrl-Q) never stops the session.

## Side Effects

- Unix-socket connection to the daemon for nearly every verb (subscribe,
  attach, input, detach, `Command::*`); `init` tries for 300 ms at most.
- stdout: confirmations, listings or JSON, session text, shell functions;
  stderr: `muster: …` messages. Exit codes carry meaning (`wait`: 0 / 3 / 4 / 5 / 124).
- `exec` spawns the agent as a child process, passes signals through, and may
  start a shell afterwards (`--then-shell`).
- `attach`/`enter`/`keys` put the person's tab in raw mode and write escape
  sequences.
- Reads stdin (prompts), the working directory, and the environment (to find
  the enclosing Muster session).
- No direct file writes: the daemon creates and removes worktrees.

## Verification

- Run: `cargo check -p muster`, then the end-to-end files for the verbs touched:
  `cargo nextest run -p muster --test <area>` (the files in
  `crates/muster/tests/` start the real daemon).
