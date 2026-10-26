# view/settings (submodules of `settings.rs`)

## Overview

The detail pane of the TUI's Settings tab, split out of
[`../settings.rs`](../settings.rs) (which draws the Settings list). Beside the
list it explains the selected setting: what it does, where its value comes
from (`SettingSource`) and where a change goes; or, with a project selected,
that project's settings as a whole.

## Interface

- `draw_setting_details(frame, app, area)` (details.rs) — draws the pane for
  `app.settings.selected`, dispatching on `SettingRow`.
- Per-row text builders (details.rs, private): `notch_text`,
  `side_by_side_text`, `colours_text`, `session_lines_text`, `panels_text`,
  `project_order_text`, `keys_text`, `notifications_text`, `agents_text`,
  `only_mine_text`, `muster_prefix_text`, `tickets_from_text`, `project_text`.
- Delegates other rows to sibling view modules: `shared_builds::text`,
  `delegation::text`, `telling::text`, and the `naming`, `look`, `panel` and
  `plugins` views.
- Uses from the parent: the row labels (`AGENTS`, `COLOURS`, `KEYS`, `NOTCH`,
  …) and value formatters (`agents_value`, `keys_value`, …) so list and pane
  read the same.

## Invariants

- Pure drawing from `App` and `muster_core::settings` types into the ratatui
  `Frame`; no IO and no state change.
- Colours come from the active `Theme` (`crate::view::theme::of(app)`).
- The notch row is explained only where the notch is known
  (`app.notch_known()`).

## Side Effects

- None beyond writing the frame buffer.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui settings`
- End to end: `cargo nextest run -p muster --test 'tui*'`
