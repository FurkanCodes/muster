# muster-tui/src

## Overview

The terminal UI for Muster. It shows what the daemon reports and sends the
person's commands back; it holds no business logic (`ARCHITECTURE.md` §3).
The code has three layers:

- **`App`** ([`app.rs`](./app.rs), [`./app/`](./app/README.md)): a state machine
  that does no IO. Keys, pastes, mouse, focus, resizes, daemon frames, clock
  ticks and animation steps go in; `Action`s come out.
- **`view`** ([`view.rs`](./view.rs), [`./view/`](./view/README.md)): pure
  drawing of an `App` into a ratatui `Frame`.
- **`runtime`** ([`runtime.rs`](./runtime.rs), [`./runtime/`](./runtime/README.md)):
  all the IO. It owns the host terminal, the link to the daemon and the
  `tokio::select!` loop, carries out the `Action`s, and is the only reader of
  the clock.

The remaining files are small helpers: input encoding for sessions, the
per-session terminal emulator, the clipboard, the terminal's colours, layout,
motion and pointer shapes.

## Interface

Public surface of the crate (`lib.rs`):

- `run(options) -> Result<(), TuiError>` (runtime.rs) — runs the UI until the
  person quits; the daemon and its sessions keep running.
- `Options` (runtime.rs) — `socket`, `connect: Option<Connector>`, `machine`,
  `start_daemon`, `cwd`, `notify`, `motion`, `start_notch` (`ARC-22`),
  `terminal_app`, `open: Option<SessionId>` (`muster attach`).
- `Connector` (runtime.rs) — boxed async connect to another machine's daemon
  (through ssh).
- `TuiError` (runtime.rs) — `Terminal(io::Error)`.
- `START_WAIT`, `START_AGAIN_AFTER` (runtime/link.rs, re-exported) — how long
  to wait for a daemon the UI started, and how often to start one.
- `keys::encode_key`, `keys::encode_paste` (keys.rs, `pub mod keys`) — a key or
  paste as the bytes a session's program expects (xterm legacy or kitty CSI u;
  bracketed paste with markers stripped).

Inside the crate:

- `App`, `Action`, `Mode`, `View`, `Dialog`, `Link` (app.rs) — UI state and the
  runtime requests it emits (`Send`, `Notify`, `Hear`, `Open`, `Copy`,
  `ListFolders`, `StartNotch`, `Activate`, `Quit`). See [`./app/`](./app/README.md).
- `perform`, `HostTerminal`, `next_wake`, `monotonic`, `wall_clock` (runtime.rs)
  — carry out actions, hold and restore the terminal's modes, schedule wakes,
  read the clock.
- `view::draw`, `view::terminal_area` (view.rs) — draw the whole window. See
  [`./view/`](./view/README.md).
- `Screen` (screen.rs) — one session's vt100 emulator (2,000 lines of
  scrollback), its terminal modes, replay tracking, DEC 2026 synchronized
  frames, and OSC 52 copies from live output.
- `mouse::encode_mouse` (mouse.rs) — a mouse event as X10/UTF-8/SGR report
  bytes, if the program's tracking mode wants it.
- `clipboard::copy`, `clipboard::from_program`, `MAX_COPY` (clipboard.rs) —
  copy to the person's clipboard; validate a program's OSC 52 payload.
- `colours::ask` (colours.rs) — reads the terminal's colours (OSC 10/11/4) from
  `/dev/tty` at start.
- `folders::list`, `Folder` (folders.rs) — sorted subfolders with a git flag,
  for the folder picker.
- `layout::Divider`, `Solved` (layout.rs) — pure panel layout with minimum
  sizes and draggable dividers.
- `motion` (motion.rs) — the 100 ms `STEP`, spinners, pulses, flashes and
  slides, drawn from a step count.
- `pointer_shape::Shape`, `Shapes::detect` (pointer_shape.rs) — OSC 22 pointer
  shapes; never inside tmux/screen; `MUSTER_POINTER=off` turns them off.
- `input::LineInput` (input.rs), `cells` (cells.rs) — one-line text field;
  display-width text measuring.
- `model` (model.rs) — re-exports `muster-model`.

## Invariants

- `#![forbid(unsafe_code)]` (`ARC-12`).
- `App` and `view` do no IO, and `layout` is pure. Only `runtime.rs` reads the
  clock (`monotonic()`, `wall_clock()`), so the app and the view are given the
  time (`ARC-03`).
- The loop never waits on the daemon. Frames from it wait in a bounded queue
  and frames to it go out through an unbounded writer (`ARC-14`). Spawned
  tasks are owned by `AbortOnDropHandle`s (`ARC-13`).
- Folder listing runs off the loop, so a slow disk cannot stop the UI
  (`ARC-14`). Nothing of this machine (folders, notch) is offered when
  `machine` names another one.
- Copied text is never logged (`PRD-06`). OSC 52 copies are taken only from
  live output, never from replays; read requests are never answered; payloads
  must be text of at most 192 KiB.
- Outside text is cleaned before it reaches the terminal or the system:
  - notification text loses its control characters;
  - `Open` takes only http(s) URLs;
  - `Activate` takes only a bundle id;
  - `Hear` takes only a bare sound name.

  Folder names with control characters are dropped.
- The host terminal's modes are restored on drop and on panic.
- The UI redraws at most once per 16 ms (`FRAME`) and holds a draw for up to
  150 ms while a session is mid synchronized frame (`ARC-13`). A mouse move
  that changes nothing draws nothing.
- Text is measured by display width, never by `char` count.
- No source file over 1,000 lines, no struct over 40 fields (`ARC-08`).

## Side Effects

- The terminal: raw mode, the alternate screen, bracketed paste, SGR
  any-motion mouse reports, focus reports, and kitty keyboard flags where the
  terminal has them. All are restored on exit.
- Escape sequences on stdout: OSC 9 notification plus BEL, OSC 52 clipboard,
  OSC 22 pointer shape.
- `/dev/tty` is read once at start for the terminal's colours (2 s deadline).
- Processes: `pbcopy` / `wl-copy` / `xclip` (OSC 52 over SSH or as a fallback),
  `open` / `xdg-open` for web links, `open -b` and `afplay` on macOS, and the
  `start_daemon` / `start_notch` callbacks.
- The daemon, over the Unix socket through `muster-client` or a `Connector`.
- Reads directories for the folder picker; reads the environment (`SSH_*`,
  `WAYLAND_DISPLAY`, `DISPLAY`, `HOME`, `MUSTER_POINTER`, terminal detection);
  listens for SIGWINCH.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui -p muster-model`
- End to end: `cargo nextest run -p muster --test 'tui*'`
