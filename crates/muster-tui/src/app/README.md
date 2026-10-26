# app (submodules of `app.rs`)

## Overview

The TUI's state machine, split by feature. [`../app.rs`](../app.rs) defines
`App`, `Action`, `Link`, `View`, `Mode` and `Dialog`. The files here add the
`impl App` blocks that handle each kind of event and each feature.

`App` does no IO. Key presses, pastes, mouse events, focus changes, folder
listings, daemon frames, animation steps and clock readings go in, and every
entry point returns `Vec<Action>` for the runtime ([`../runtime.rs`](../runtime.rs))
to carry out. This keeps all behaviour testable: a test feeds events and draws
into a test buffer.

Of the directory's 98 files, 45 are production code. The rest are 52
`*_tests.rs` unit-test modules and `tests.rs`. Most are declared in `app.rs`;
five are declared by `#[path]` in their feature's file (rename.rs,
spaces.rs, stacks.rs, tidy.rs, timeline.rs).

## Interface

Entry points (all on `App`):

- `on_key`, `on_paste` (events.rs) — key entry. It ignores releases, skips the
  welcome, clears hover and any error note, then routes the key through the
  panel prefix and the current `Mode`. `screen_key` makes Ctrl+1/2/3 switch tabs
  from anywhere; `key_screen` and `does` resolve a chord through the person's
  keymap.
- `on_mouse_at(event, ms)`, `on_mouse` (pointer.rs) — mouse entry; `ms` from the
  runtime times double-clicks (400 ms).
- `on_connected`, `on_disconnected`, `on_frame`, `on_focus` (link.rs) — the
  daemon link and every `ServerFrame`, including request replies (`on_reply`).
- `on_resize` (app.rs), `on_folders` (picker.rs), `on_step`, `on_clock`
  (motion.rs) — window size, folder listings, animation steps, wall time.
- `settle` (settle.rs) — runs after any change. It keeps selections valid and
  in view, leaves Terminal mode for a gone session, and lines up fetches,
  streams and subscriptions with what is shown.

State and features:

- `View`, `Mode`, `Dialog`, `Confirmed`, `Action`, `Link` (app.rs) — tab, key
  target (`Browse`, `Terminal`, `Changes`, `Dialog`, `Menu`, `Help`), modal
  forms, and runtime requests.
- `panel_keys`, `Resizing` (prefix.rs) — the panel prefix key (Ctrl-B by
  default) and the key that follows it: split, move, focus, zoom, close,
  place, resize, or go to a panel by number.
- `Here`, `key` (actions.rs) — what can be done on the selected line, shared by
  the bottom-bar hints and the right-click menu.
- `NeedsScreen`, `TicketsScreen` (screens.rs) — per-screen UI state kept apart
  from the daemon's model.
- Tickets: `Tickets`, `TicketRow`, `ticket_sources` (tickets.rs), sharing.rs,
  stacks.rs, tidy.rs, tickets_from.rs.
- Spaces: `Choice`, `Later` (spaces.rs, `ARC-33`).
- Changes: `ChangesPane`, `DiffPane`, `ChangeRow`, `on_changes`,
  `on_timeline`, `on_diff` (changes.rs); `ask_timeline`, `ask_undo_plan`,
  `on_undo_plan`, `undo_key`, `on_undone` (timeline.rs).
- Panels and layout: `Drag` (drag.rs), grab.rs, panels.rs, placed.rs,
  shell.rs, narrow.rs, fold.rs, project_order.rs, scrollbars.rs, lines.rs.
- Plugins: `PluginKey`, `PluginDialog` (plugins.rs), plugin_panel.rs.
- Settings: `Settings`, `SettingRow` (settings.rs), look.rs, telling.rs,
  naming.rs, `Rebinding` (rebind.rs), notch.rs.
- Mouse UI: controls.rs, dialog_mouse.rs, `Menu` (menu.rs), selection.rs.
- Dialog helpers: `HarnessChoice` (harness_choice.rs), `FolderPicker`
  (picker.rs), cloning.rs, rename.rs; `Help` (help.rs); motion.rs
  (`motion_step`, `animating`), alarm.rs (`on_news`), welcome.rs.

## Invariants

- No IO, and no clock reads. Time comes from the runtime as steps, `ms` and
  `on_clock` (`ARC-03`).
- Keys resolve through the person's keymap (`muster_core::keys`), never
  hard-coded; hints, menus and help name the same keys. The panel prefix works
  only in Browse, Terminal and Changes. Settings is plain `4` on the list,
  because without the kitty protocol Ctrl+4 is the same byte as Ctrl+\\.
- On connect or disconnect, per-daemon state is reset: streams, pending
  requests and terminal screens.
- Terminal-output frames skip `settle`. `settle` fetches and streams only
  while `Link::Connected`.
- Folds and the space shown belong to this UI and are never told to the
  daemon. The selection rests only on a drawn line.
- Tickets are read-only, except closing one after asking. A repository's own
  GitHub/GitLab issues are never shared between projects.
- Setting changes are written by the daemon into the person's settings, never
  into a project's repository (`ARC-05`).
- A plugin add or update shows everything it would run before the person
  agrees (`PRD-15`, `PRD-01`).
- Click targets are one list that the view lights and the mouse presses
  (`ARC-32`). A dialog click presses the keys that choose, so the dialog's own
  rules decide.
- A session the narrow list does not show is not streamed, so nothing it does
  counts as seen.
- No file over 1,000 lines and no struct over 40 fields (`ARC-08`): the reason
  `App` is split this finely.

## Side Effects

- None directly; everything is in memory. Effects are returned as `Action`s:
  - `Send(ClientFrame)`: subscribe, colours, focus and looking state,
    terminal input (`encode_key` / `encode_paste` / `encode_mouse`), resizes,
    and `Command` requests (changes, diffs, tickets, settings, plugins,
    layout, rename, clone, undo, tidy, restack, spaces);
  - `Notify`, `Hear(Sound)`, `Open(url)`, `Copy(text)`, `ListFolders(dir)`,
    `StartNotch` (`ARC-22`), `Activate`, `Quit`.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui <filter>`
  (the `*_tests.rs` here)
- End to end: `cargo nextest run -p muster --test 'tui*'`
