# plugins (submodules of `plugins.rs`)

## Overview

The two output formats of `muster plugin` (`ARCHITECTURE.md` §7a, `PRD-15`).
The command itself is in the parent [`../plugins.rs`](../plugins.rs): the clap
`PluginCommand` (list, install, link, enable, disable, update, remove, log,
hook), its confirmation prompts, and the `Command::Plugin(..)` calls it sends
to the daemon. This directory turns the daemon's replies into the `--json`
document (`json.rs`) and into human-readable text (`shown.rs`).

## Interface

- `print` (json.rs) — prints a `PluginList` as one JSON document through
  `crate::json::print`, with `format: FORMAT`.
- `Document`, `PluginEntry`, `PanelEntry`, `TicketsEntry`, `SettingEntry`,
  `HookEntry`, `WhenEntry`, `RequiredEntry`, `ChangeEntry` (json.rs, private) —
  the `Serialize` structs that make up the stable JSON schema.
- `entry`, `panels`, `tickets`, `hooks`, `requires` (json.rs) — map protocol
  types onto those structs field by field.
- `list` (shown.rs) — every plugin as text, with a notice when the settings do
  not read and a hint when there are none.
- `plugin` (shown.rs) — one plugin: its state (on / on, not running / off),
  version, source, problem, what it runs, and its last change.
- `preview` (shown.rs) — the install/link/update preview: name, version, id,
  source, current vs new manifest, minimum Muster version, everything it runs,
  manifest SHA-256, and missing-program warnings.
- `tried` (shown.rs) — results of `muster plugin hook`: outcome, duration and
  cleaned output.
- `approved` (shown.rs) — the line printed after approval.

## Invariants

- The JSON is built from the daemon's reply, never by serialising protocol
  types, so the protocol can change without breaking scripts. Its string enums
  are fixed: source `builtin|git|folder`; problem
  `changed|not_approved|missing|unreadable|unknown`; changed
  `installed|linked|updated|turned_on|turned_off`; by `ui|command_line`.
- A preview lists every panel, ticket source, hook, required program,
  environment variable and secret name the plugin would use, before the person
  is asked (`PRD-15`).
- Everything from a plugin (names, words, commands, hook output) is shown
  without terminal control characters (`PRD-07`).
- The JSON carries no secrets (`PRD-06`), only secret names.

## Side Effects

- `json::print` writes to stdout.
- `shown.rs` is pure: it returns `String`s for the parent to print.

## Verification

- Run: `cargo check -p muster`, then `cargo nextest run -p muster --test plugins`
  (end-to-end against the real daemon; see `crates/muster/tests/`).
