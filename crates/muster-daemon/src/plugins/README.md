# plugins

## Overview

The submodules of `src/plugins.rs`, the registry of plugins the person
installed (`PRD-15`, `ARCHITECTURE.md` §7a). The Supervisor owns the
`Registry` (`ARC-02`); it is read from Muster's own settings and the plugins'
folders under `<data>/plugins`. Nothing of a plugin runs while it is fetched or
listed: git fetches it and Muster reads its manifest. Each manifest is hashed
as the registry is read; one that differs from the approved hash is listed as
changed and cannot run until approved again.

This directory holds the changes the person asks for (preview, install, link,
update, enable, remove), fetching off the Supervisor's loop, the on-disk
layout, the previews held for approval, and plugin hooks.

## Interface

From the module root `src/plugins.rs`:

- `Registry` (plugins.rs) — the plugins, owned by the Supervisor: `read`,
  `take`, `infos`, `list`, `ticket_plugins`, `runnable`, `hook_jobs`.
- `PluginError` (plugins.rs) — why a plugin change or run was refused;
  `body()` gives the client-facing `ErrorBody` (`ARC-07`).
- `BUILTINS` / `Builtin` (plugins.rs) — Muster's own plugins, served by the
  same protocol as anyone's (`ARC-23`).
- `start`, `read`, `read_manifest`, `sha256` (plugins.rs) — read the registry
  and manifests, sweep folders no entry names (`ARC-14`).
- Crate-private helpers (plugins.rs): `Reading`, `job()`, `on_path()`, and
  `Registry::running_ids` / `folders` / `incoming`.

In this directory:

- `Registry::hold` / `install` / `set_enabled` / `remove` / `update_job`
  (change.rs) — the person's changes, made on the Supervisor's loop; `Gone`
  is the list of folders to remove off it.
- `Job`, `Found`, `Fetched`, `fetch` (fetch.rs) — clone a git repository into
  `.incoming`, or read a local folder, to preview it; no build step runs.
  `Fetched` is the result handed back to the Supervisor's loop.
- `incoming`, `private_dir`, `place`, `remove`, `remove_later`, `sweep`
  (files.rs) — the `<data>/plugins` layout: `.incoming`, `<id>/<commit>/`,
  folders 0700.
- `Previews`, `MOST` (8), `FOR` (10 minutes) (previews.rs) — previews held
  under a random id; one let go has its clone removed.
- `Hooks`, `Job`, `Limits` (hooks.rs) — hook runs: at most 4 per plugin at
  once, 32 waiting (oldest dropped), deadline and output cap; `raise`,
  `publish`, `cut`, `reopen`.
- Hook runs themselves: [hooks/README.md](./hooks/README.md).

## Invariants

- No code of a plugin runs while fetching, previewing or listing: no build or
  install step (`PRD-15`).
- A plugin whose manifest hash differs from the approved one is "changed" and
  runs nothing until approved again (`PRD-15`).
- Changes are made on the Supervisor's loop, so two never interleave in
  Muster's settings (`ARC-02`).
- Everything is bounded (`ARC-14`): at most 8 previews for 10 minutes each;
  hooks at most 4 running and 32 waiting per plugin.
- A plugin turned off, removed or no longer approved has its waiting hook runs
  dropped and its running ones cut.
- A linked folder belongs to its author and is never modified or removed.
- `hook_jobs` raises a run of each hook that takes the event, told whether the
  person is `away` from every Muster window (`PRD-15`).

## Side Effects

- Runs `git` to clone plugin repositories into `<data>/plugins/.incoming`.
- Creates, moves and removes folders under `<data>/plugins` (mode 0700).
- Writes Muster's settings (the plugin entries and approved hashes).
- Spawns hook programs (see `hooks/`), appends to each plugin's log.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon plugins`
- End to end: `cargo nextest run -p muster --test plugins --test hooks --test ticket_plugins`
