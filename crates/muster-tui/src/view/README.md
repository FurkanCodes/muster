# view (submodules of `view.rs`)

## Overview

The TUI's drawing layer. [`../view.rs`](../view.rs) is pure: it reads the
`App` and writes the ratatui `Frame`. `view::draw` works in this order:

1. With `app.welcome` set, it draws the welcome screen (Kep) and stops.
2. With a session being typed into full screen, it draws that terminal and
   stops.
3. Otherwise it paints the theme over the frame, lays out the window with
   `browse_areas`, and draws the top bar, the current tab's list (Sessions,
   Needs you, Tickets or Settings), the preview and pinned panels, and the
   bottom bar.

The files here split that work by screen or widget. Most return
`Line`/`Span`/`Text` or render widgets into the frame. Each layout function is
shared with the mouse code, so a click lands where a thing is drawn.

## Interface

- `draw`, `browse_areas`, `BrowseAreas`, `session_area`, `panel_area`,
  `terminal_area`, `help_area`, `line_hints` (view.rs) — the entry point and
  the window layout given to the input code and to sessions' programs.
- `Key` (areas.rs) — cache key for the window layout (`app.areas_cache`).
- `draw_top_bar`, `TopControl` (top_bar.rs) — Kep's face, tabs, agent counts,
  memory bar and daemon version; all clickable.
- `draw_bottom_bar`, `bottom_height`, `hint_areas`, `hint` (bottom_bar.rs) — key hints,
  the last note, unreachable daemon, version warning; a click on a hint presses
  its key.
- `draw_sessions`, `draw_needs_you`, `list_width` (sessions.rs);
  `space_name`, `space_line` (spaces.rs, `ARC-33`) — the session lists and
  their space headings.
- `draw_tickets`, `details_height` (tickets.rs); `lines` (markdown.rs);
  `badge` (pull_request.rs) — the Tickets tab, ticket Markdown, PR badges.
- `draw_settings`, `hints` (settings.rs) — the Settings list; the detail pane
  is in [`./settings/`](./settings/README.md). Settings row texts:
  `delegation.rs`, `shared_builds.rs`, `naming.rs`, `telling.rs`, `plugins.rs`.
- `DialogPress`, `lay_out`, `input_window` (dialogs.rs) — every dialog over
  the list, from one layout that drawing and mouse both read; the "Tickets
  from" dialog is in [`./dialogs/`](./dialogs/README.md). The plugin
  add/update dialog is in `plugin_dialog.rs`.
- `draw_preview` (preview.rs); `draw_session`, `draw_terminal` (terminal.rs,
  `tui_term` `PseudoTerminal`); `draw_selection` (selection.rs) — the pane
  beside the list, a session's terminal, and its text selection.
- `draw_changes` (changes.rs); `mark` (clashes.rs) — a workspace's changes
  and timeline or one diff; conflict and overlap marks.
- `centred` (floating.rs), `draw_help` (help.rs), `menu_area` (menu.rs),
  rebind overlay (rebind.rs), `Button` (pane.rs), `draw_moving` (drag.rs),
  `track` (scrollbar.rs) — floating boxes, keys help, right-click menu, key
  rebinding, panel buttons, drag highlights, scrollbars.
- `draw_welcome` (kep.rs), `rule` (loading.rs), `stale_warning` (version.rs)
  — the welcome, the one loading indicator, and the older-UI warning.
- `Theme`, `of` (theme.rs); `project_colour`, `edge`, `WORDMARK` (look.rs) —
  the palette and the shared look.
- `Graphics::detect` (logos.rs), `Backdrop` (picture.rs) — agent logos as
  glyphs or iTerm2 inline pictures.

## Invariants

- Pure drawing: reads `App`, writes `Frame`. No IO, and no clock reads:
  animation uses `app.motion_step()` (`ARC-03`).
- Drawing and mouse share one layout function per widget: dialogs `lay_out`,
  `hint_areas`, the top-bar controls, `menu_area`, pane buttons and scrollbars
  (`ARC-32`).
- The layout cache is keyed by every layout input and never by output or
  animation. It holds two entries and is owned by the app (`ARC-14`).
- While a session beside the list has the keys, the bottom bar keeps to one
  line, so the session keeps its size.
- Text from outside (paths, commit subjects, plugin names and commands,
  tickets) is drawn on one line with control characters removed. Ticket
  Markdown opens and loads nothing, and links are shown as words plus their
  address (`PRD-07`).
- Theme: the Muster palette takes its shades from the terminal's own
  background, never a grey of Muster's choosing; the `terminal` palette uses
  the terminal's 16 colours. Whatever has the keys is drawn in the accent.
- Logo pictures only in iTerm2/WezTerm, never inside tmux/screen;
  `MUSTER_LOGOS=text|pictures` overrides. A picture escape is re-sent only when
  its line changes. The picture cache holds at most 32 (`ARC-14`).

## Side Effects

- Writes only into the ratatui buffer. `logos.rs` places iTerm2 `OSC 1337`
  inline-image escapes into cells (`ForcedWidth`); they reach the terminal
  through ratatui's normal flush.
- Process-wide caches: `KEPT_PICTURES` (picture.rs) and `SEQUENCES` (logos.rs).
  Logo bytes are compiled in from `../../assets/logos`.
- `areas()` memoises into `app.areas_cache` through a `RefCell`.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui view`
- End to end: `cargo nextest run -p muster --test 'tui*'`
