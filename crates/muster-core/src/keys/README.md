# keys submodules

## Overview

Holds the submodules of [`../keys.rs`](../keys.rs). That file defines the keys
of Muster's screens (`PRODUCT.md` §6). Each thing a key does is a named
`Action`, with the screens it works on, its default keys and a description.
The person can rebind any action under `[keys]` in Muster's own settings.

The one production file here adds the keys of plugin panels. `export.rs` is
built only for `cfg(all(test, feature = "ts"))`, and `tests.rs` holds only
unit tests.

## Interface

- `Action`, `Screen`, `Chord`, `Key`, `Keymap`, `KeysError` and `KeysWarning`
  (`../keys.rs`):
  - `Chord::parse` reads a key name such as `ctrl-c`, and `Chord::label` shows
    it.
  - `Keymap::with` builds a keymap from the `[keys]` table.
  - `Keymap::action(screen, chord)` finds the action a key does.
  - `check`, `taken`, `labels`, `aside` and `warnings` check a binding, find
    what already uses a key, show labels, and report names put aside.
- `PanelAction`, `panel_action(plugin, panel)` and `is_panel_action`
  (panels.rs): a plugin panel's action, named `plugin.<plugin>.<panel>`.
- `Keymap` methods for panels (panels.rs): `with_panels` adds them.
  `check_panel` checks a binding. `panel` finds a panel by key, and
  `panel_named` finds one by name. `panels` lists them. `panel_label`,
  `panel_labels`, `panel_title`, `panel_what` and `panel_is_set` give their
  labels, title, description and whether the person set the key.
- `export_bindings_keys` (export.rs, test-only): writes `keys.json` and
  `ActionName.ts` for the desktop app when `TS_RS_EXPORT_DIR` is set
  (`ARC-24`).

## Invariants

- Moving about (the arrows, page keys, ⏎) and typing in a field are never
  actions. They are the same on every screen, and no action may take them.
- A name in `[keys]` that is not an action, such as the panel of a plugin that
  was removed, is put aside and warned about. It is never refused, so the file
  still reads.
- A panel's suggested key is dropped with a warning if it is already used on
  the session lists, is one of the keys every screen keeps, or belongs to
  another panel.
- Keys are bound in a fixed order: the person's own keys first, then the
  plugins' suggestions in the order the caller gives them (`with_panels` does
  not sort them). A plugin added later never takes a key the person gave to
  another plugin's panel.
- Parsing does no IO (`ARC-01`).

## Side Effects

- None in production code. The test-only `export.rs` writes binding files
  into `TS_RS_EXPORT_DIR`, and only when `scripts/bindings.sh` runs the tests
  with the `ts` feature.

## Verification

- Run: `cargo check -p muster-core`, then
  `cargo nextest run -p muster-core keys`.
- Run `pnpm bindings` in `apps/desktop` after changing an action's name.
