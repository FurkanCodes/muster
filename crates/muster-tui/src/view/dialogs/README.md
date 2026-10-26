# view/dialogs (submodules of `dialogs.rs`)

## Overview

Dialog shapes split out of [`../dialogs.rs`](../dialogs.rs), the TUI's dialog
drawing. Today it holds one: the dialog that chooses where a project's tickets
come from (its git remote or a ticket-source plugin) and takes that source's
settings.

## Interface

- `tickets_from(app, project, tracker, fields, at) -> Shape` (tickets_from.rs) —
  builds the "Tickets from" dialog: a row of chips (`its git remote` plus each
  of `App::offered_trackers()`), the chosen source's description, and one typed
  field per setting (`(if you like)` for optional ones), with the field at `at`
  holding the cursor and the rest below it.
- Uses from the parent: `Shape`, `Mark`, `Where`, `DialogPress`
  (`Tracker(..)`, `Field(index)`), `chips`, `typed_line`.

## Invariants

- Pure drawing: reads `App` and returns a `Shape`; no state changes and no IO.
- Every chip and every field line carries a `Mark` with its `DialogPress`, so a
  mouse press on it does what the keys do.
- The hint line names only the keys that apply: `↑↓ field` only with more than
  one field.

## Side Effects

- None.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui tickets_from`
- End to end: `cargo nextest run -p muster --test 'tui*'`
