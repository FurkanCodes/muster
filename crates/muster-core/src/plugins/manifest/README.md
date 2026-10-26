# plugins::manifest submodules

## Overview

Holds the submodules of [`../manifest.rs`](../manifest.rs), the strict reader
of a plugin's `muster-plugin.toml`. The one production file here reads the
manifest's `[[hooks]]` tables. It is added to the private `Read` helper that
`manifest.rs` defines, so it shares that file's strict rules and error type.
Text goes in and `Hook` values come out. No IO (`ARC-01`).

`./hooks/` and `tests.rs` hold only unit tests (`#[cfg(test)]`). They are not
documented here.

## Interface

- `Read::hooks` (hooks.rs), `pub(super)`: reads every `[[hooks]]` table into a
  `Vec<crate::plugins::hooks::Hook>`, or returns a `ManifestError` that names
  the key at fault.
- `HOOK_KEYS` (hooks.rs), private: the keys a hook table may have (`id`, `on`,
  `command`, `timeout`, `when`).
- `WHEN_KEYS` (hooks.rs), private: the filter keys under `when` (`project`,
  `harness`, `reason`, `away`).
- Private helpers `events`, `hook_placeholders` and `when` (hooks.rs): read the
  event names, check the placeholders in a command, and read the filters.

Nothing in this directory is public outside `plugins::manifest`. The parent
module re-exports `Manifest` and `ManifestError` (see
[`../README.md`](../README.md)).

## Invariants

- Reading is strict (`PRD-15`). An unknown key, a missing key or a value of the
  wrong kind is an error that names the key, for example `hooks[0].when`. It is
  never replaced by a default.
- A hook `id` must pass `crate::plugins::is_id`: 1 to 32 characters from a-z,
  0-9 and `-`, starting with a letter or digit. Ids are unique within a
  manifest (`ManifestError::Twice`).
- `on` names events only by their `EventKind::named` names.
- `timeout` defaults to `hooks::TIMEOUT` (10 s) and may be no more than
  `hooks::MOST_TIMEOUT` (60 s).
- A session placeholder is refused if any of the hook's events is not a
  session's (`ManifestError::NoSession`). The `harness` and `reason` filters
  are refused only if none of the hook's events could carry them. Only
  placeholders that `Placeholder::is_told_to_hooks` allows are accepted
  (`PRD-06`, `PRD-07`).

## Side Effects

- None. This is a pure parser over a TOML value that is already in memory.

## Verification

- Run: `cargo check -p muster-core`, then
  `cargo nextest run -p muster-core plugins::manifest`.
