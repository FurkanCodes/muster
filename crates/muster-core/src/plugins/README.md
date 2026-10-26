# plugins

## Overview

The pure half of Muster's plugins (`PRD-15`, `ARCHITECTURE.md` §7a). A plugin
is a folder with a `muster-plugin.toml` and the programs the daemon runs out of
process (`ARC-23`). This module covers the parts that need no IO:

- reading a manifest;
- the events hooks are told of, and which events a hook takes;
- filling in a panel command's placeholders;
- the person's registry of installed plugins;
- the manifests of Muster's own built-in ticket plugins.

The daemon fetches, hashes and moves the files, and runs the programs. The
module root is [`../plugins.rs`](../plugins.rs). This directory holds its
submodules.

`./events/` holds only the unit tests of `events.rs`, so it has no README.

## Interface

- [`./manifest/README.md`](./manifest/README.md): the `[[hooks]]` reader inside
  `manifest.rs`.
- `Manifest`, `Panel`, `PanelOn`, `TicketSource`, `TicketSetting` and
  `ManifestError` (manifest.rs): `Manifest::parse` and `Manifest::parse_own`
  turn the text of a manifest into a `Manifest`. `FILE_NAME` is
  `muster-plugin.toml` and `MOST_BYTES` is 64 KiB. `Manifest::commands` lists
  every command a manifest would run.
- `Event`, `EventKind` and the `Event*` parts (events.rs): what a hook is told,
  as a single JSON object on its stdin. `EventKind::ALL` lists the 10 kinds.
  `EVENT_VARIABLE` is `MUSTER_PLUGIN_EVENT`. `Event::sample` builds an example
  event.
- `Hook`, `When`, `TIMEOUT`, `MOST_TIMEOUT`, `REASONS` and `reason_name`
  (hooks.rs): `Hook::takes(event, away)` says whether a hook runs for an event.
- `Placeholder`, `Context` and `Missing` (placeholders.rs): `{branch}`,
  `{transcript}` and the other placeholders. `Context::expand` fills in a
  command's arguments, and `Context::variables` gives the matching environment
  variables.
- `PluginEntry`, `PluginSource`, `RegistryError` and `registry::parse`
  (registry.rs): the `[plugins.<id>]` tables in `settings.toml`.
- `own::MANIFESTS` (the compiled-in manifest texts), `own::manifests`,
  `own::manifest(id)`, `own::PROGRAM` and the ids `GITHUB`,
  `GITLAB`, `JIRA` and `LINEAR` (own.rs): the built-in ticket plugins. Their
  manifests are compiled in and run as `muster plugin-run <id>`.
- `is_id`, `is_reserved`, `is_commit`, `is_sha256`, `is_git_ref`,
  `check_address`, `AddressError`, `ID_MAX`, `RESERVED` and `MUSTER_VERSION`
  (`../plugins.rs`): checks on ids, refs and addresses.

## Invariants

- Manifests and the registry are read strictly. An unknown key or a value of
  the wrong kind is an error that names the key, never a default, because what
  a manifest says is what the person approves (`PRD-15`).
- The ids `github`, `gitlab`, `jira` and `linear` (`RESERVED`) belong to
  Muster's own plugins, and `is_reserved` also keeps `muster` and every id
  starting with `muster-`. Another plugin may not use them.
- An event carries names and ids only. It never carries a ticket's text, a
  prompt, an agent's output or code. A chat's name has its credentials hidden
  (`PRD-06`, `PRD-07`).
- A hook only hears. Nothing it returns can start, prompt or type into a
  session (`PRD-01`).
- A plugin address that holds a password is refused (`check_address`,
  `ARC-20`).
- A placeholder is `{` followed by a lowercase word and `}`. Any other word in
  braces is refused when the manifest is read, so `Context::expand` only ever
  finds known placeholders.

## Side Effects

- None. Text and in-memory values go in and values come out (`ARC-01`). The
  daemon reads files and runs programs.

## Verification

- Run: `cargo check -p muster-core`, then
  `cargo nextest run -p muster-core plugins`. The plugin wire types are
  round-tripped against `crates/muster-core/tests/plugin_samples` by
  `--test protocol_round_trip`.
- End to end: `cargo nextest run -p muster --test plugins --test ticket_plugins --test tui_plugins`.
