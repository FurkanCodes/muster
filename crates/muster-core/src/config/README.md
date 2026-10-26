# config submodules

## Overview

Holds the submodules of [`../config.rs`](../config.rs). That file reads a
project's settings (`ARCHITECTURE.md` §12): the repository's `muster.toml`,
with the person's local overrides merged over it key by key. The files here
cover:

- coding agents the person adds;
- editing the overrides from the UI while keeping comments;
- Muster's own whole-app `settings.toml`;
- where a project's tickets and pull requests live.

Everything is text in and values or text out. The daemon reads and writes the
files (`ARC-01`). `tests.rs` holds only the unit tests of `config.rs`.

## Interface

- `ProjectConfig`, `Branches`, `Checks`, `Agents`, `RunLimits`, `Intake`,
  `ConfigError` and `NoChecks` (`../config.rs`):
  `ProjectConfig::parse(repository, local, names)` merges and reads the two
  files. `ProjectConfig::ready_for_runs` refuses to start runs when no checks
  are set (`PRD-02`). `parse_duration` reads values such as `1h30m`, and
  `parse_size` reads values such as `512m`, `1.5g` or a plain byte count.
- `AgentDefinition`, `TitleWords`, `TitleSays` and `PROMPT` (agents.rs): the
  `[agents.<name>]` entries in `settings.toml`. `AgentDefinition::arguments`
  builds a command line. `takes_prompt`, `can_resume` and `asking` describe
  what the agent supports. `TitleWords::read` reads a status from the window
  title. `AgentDefinition::bare` stands in for an agent that is no longer
  defined, and `AgentDefinition::added` marks one the person added.
- `agents::parse` and `AgentError` (agents.rs): read the `[agents.*]` tables;
  `AgentError` is what `ConfigError::Agent` carries.
- `set_flag`, `set_in`, `set_list_in`, `set_texts` and `set_texts_in`
  (edit.rs): set values in the person's overrides.
- `remove_in`, `remove_table_in`, `set_entry_in` and `remove_entry_in`
  (edit.rs): remove values and tables, or set and remove whole entries.
- `keys_of`, `source_of`, `Written`, `EditError` and `NEW_FILE` (edit.rs): list
  a table's keys and say where a value comes from. `source_of` returns a
  `crate::settings::SettingSource` (local, over repository, over default).
- `MusterConfig`, `FILE_NAME` (`settings.toml`) and `NEW_FILE`
  (preferences.rs): `MusterConfig::parse(text, file)` reads the `[notch]`,
  `[agents.*]`, `[layout]`, `[look]`, `[notifications]`, `[keys]`, `[names]`,
  `[worktrees]` (`shared_builds`), `[delegation]` (`allowed`, `at_once`,
  `depth`) and `[plugins.*]` tables.
- `Tickets`, `PullRequests`, `HostKind`, `TrackerId`, `tracker_name` and
  `check_fields` (services.rs, a private module re-exported from
  `config`): the `[tickets]` and `[pull_requests]` tables. A tracker's own
  keys are checked against its plugin manifest's `[[tickets.settings]]`.

## Invariants

- Reading is strict (`ARC-05`). An unknown key or a value of the wrong kind is
  an error that names the key, never ignored. A misspelt `comands` would
  otherwise leave runs with no checks (`PRD-02`).
- Only the person's own `settings.toml` may define an agent, never a
  repository. An agent name that one of Muster's own harnesses uses is refused
  (`ARC-19`).
- UI changes go into the person's overrides, never into the repository's
  `muster.toml` (`ARC-05`). Edits go through `toml_edit`, so the rest of the
  file, comments included, stays as the person wrote it.
- Muster never writes into an added agent's own config to hear more from it.
  Its status comes from the terminal progress sequence, the window title and
  its exit (`PRD-13`).

## Side Effects

- None. Every function takes and returns text or values. The daemon does the
  file IO (`ARC-01`).

## Verification

- Run: `cargo check -p muster-core`, then
  `cargo nextest run -p muster-core config`.
