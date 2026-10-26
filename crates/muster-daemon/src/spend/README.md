# spend

## Overview

The submodules of `src/spend.rs`: what each session's agent has used
(`PRD-12`), read from the agent's own records, which Muster only reads
(`PRD-13`): Claude Code's transcripts, Codex's rollouts and Gemini CLI's chat
recordings. Every few seconds the reader asks the Supervisor which sessions to
read, reads what each record has gained, off the Supervisor's loop, and hands
back the totals that changed. OpenCode's plugin reports its own spend
(`harness::opencode`); Copilot CLI's sessions report none.

Claude Code's transcript also tells when the person cut a turn short and when a
background task (a subagent) stopped, which no hook does; both are handed back
so the Supervisor can end the turn or let go of the subagent.

## Interface

From the module root `src/spend.rs`:

- `run(supervisor, interval, places)` (spend.rs) — the reader task.
- `Readers` (spend.rs) — per-session readers: `read(targets, now)`,
  `take_interrupted`, `take_stopped`.
- `Target`, `Interrupted`, `Stopped` (spend.rs) — what to read; turns cut
  short (by prompt id); tasks stopped (by id).
- `Places` (spend.rs) — where each agent keeps its records; `from_env()`.
- `readable(harness)`, `transcript(places, harness, agent_session)`
  (spend.rs) — whether and where a harness's record can be read.

In this directory:

- `Claude` (claude.rs) — tokens from each assistant message's `usage`,
  counted once per message id, subagent transcripts included; interrupt and
  `<task-notification>` detection.
- `Codex` (codex.rs) — the last `token_count` event's total.
- `Gemini` (gemini.rs) — the sum of the model messages' `tokens.total`; the
  whole JSON re-read when it changes.
- `Tail` (tail.rs) — lines appended to a file since the last read, a chunk at
  a time.

## Invariants

- Agents' records are only read, never written (`PRD-13`).
- Totals are kept nowhere: after a restart each record is read again from its
  start.
- No cost is reported for Claude Code, Codex or Gemini CLI, which record none
  (`PRD-12`).
- From a transcript only ids are kept (prompt ids, task ids), nothing of the
  conversation (`PRD-06`).
- Claude Code's and Codex's records are read in chunks; only a chunk and the
  current line are held (`ARC-14`). Gemini CLI's recording is one JSON file,
  read whole (`std::fs::read`) each time it changes.

## Side Effects

- Reads files under the agents' own directories: `$CLAUDE_CONFIG_DIR/projects`
  or `~/.claude/projects`, `$CODEX_HOME/sessions` or `~/.codex/sessions`,
  `~/.gemini/tmp`.
- Sends spend, interrupts and stopped tasks to the Supervisor.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon spend`
- End to end: `cargo nextest run -p muster --test claude --test harnesses`
