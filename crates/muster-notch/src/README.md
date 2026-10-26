# muster-notch/src

## Overview

Muster's notch (`ARC-22`): on a MacBook with a notch, the agents shown at
it while the person looks away from every Muster window. Counts sit in a
slim side either side of the notch; pointed at, the panel opens downwards
into the list of sessions, and a click shows one in the Muster window looked
at last. The notch is a daemon client like the UI: it learns everything from
the daemon's state and changes nothing except through commands.

The crate splits into pure, testable-anywhere modules (`board`, `trouble`,
`geometry`, `hover`, `mirror`) and macOS-only ones (`link`, `mac`, and
`news`, which is pure but compiled only for the Mac).

## Interface

- `run(socket, runtime_dir, relaunch)` (lib.rs) — runs the notch until turned off or the daemon is gone; returns at once if another notch holds the lock, or if no screen has a notch and the program is not running as Muster's app (inside the app it carries on without a notch, so notifications still work); must run on the main thread.
- `NotchError` (lib.rs) — `NotAMac`, `NotMainThread`, `Io`.
- `MOVED_FOR` (lib.rs) — env var `MUSTER_NOTCH_FOR` set on a notch relaunched for a daemon of another version, so it is not restarted over and over.
- `Board`, `Row`, `Mark`, `MOST_ROWS` (board.rs, pub) — what the notch shows: `Board::of(state, connected, now)`, `hidden`, `asks_anew`, `fresh`, `first_working` (and crate-private `next_change`); `ago(then, now)` words a duration.
- `troubles`, `work`, `Trouble`, `Key` (trouble.rs, pub) — conflicting workspaces and failed PR checks; a session's work summary (`+120 −30  #88 ✗`).
- `find`, `Notch`, `Screen`, `Rect`, layout constants `SIDE`, `LINE`, `PAD`, `LIST_WIDTH`, ... (geometry.rs, pub) — where the notch and panel are, in AppKit screen coordinates; rest/open rects, `line_at`, `over`, and `outline`, which returns `Segment`s.
- `Hover`, `OPEN_AFTER`, `CLOSE_AFTER` (hover.rs, pub) — open after 200 ms resting on the notch, close 300 ms after leaving; counted in 100 ms steps.
- `mirror::apply` (mirror.rs, crate) — applies a `Delta` to the notch's `Snapshot` copy.
- `News`, `Posted`, `posts`, `HOLD` (news.rs, crate, macOS) — what newly needs the person, for Mac notifications.
- `link::run`, `ToScreen`, `FromScreen`, `Ended` (link.rs, crate, macOS) — the daemon connection; asks `Command::Reveal` for a picked session.
- `mac::run` (mac.rs, crate, macOS) — the AppKit panel and its drawing; submodules in [mac](./mac/README.md).

## Invariants

- At most one notch runs: it holds an exclusive lock on `notch.lock` in the runtime directory for its whole life.
- `board`, `trouble`, `geometry`, `hover` and `mirror` do no IO and no AppKit, so they are tested on any OS; `news` is pure except for the `Instant` it is handed.
- The notch shows only while no Muster window is looked at, an agent works or waits, the daemon is connected, and the settings have it on.
- The link never starts the daemon; after the daemon has been gone for `PATIENCE` (30 s) the notch ends.
- A daemon of another version makes the notch relaunch onto the installed program (`relaunch`), unless `MOVED_FOR` already names that version.
- AppKit is used from the main thread only; the link runs on its own thread with a current-thread tokio runtime and hands boards over on the main queue.
- The only `unsafe` code in the crate is in `mac.rs` and `mac/` (`ARC-12`).
- The clock is reached in one place each: `link::wall_clock` (wall time) and `mac::events::now` (monotonic) (`ARC-03`); hidden boards have no timer, and time labels wake only at their next boundary.
- A snapshot is never news; a finish is told only after it holds for `HOLD`, and only kinds the person chose are notified.

## Side Effects

- Creates/locks `<runtime_dir>/notch.lock`.
- Connects to the daemon socket via `muster-client`: subscribes (`ClientFrame::Subscribe`), says whether it can notify (`ClientFrame::Notifier`), and sends `Command::Reveal`.
- Draws an AppKit panel, installs event monitors, posts Mac notifications (see `mac/`).
- Spawns a thread for the link; may relaunch the program or call `std::process::exit(0)` when the link ends.

## Verification

- Run: `cargo check -p muster-notch`, then `cargo nextest run -p muster-notch`
- End-to-end: `cargo nextest run -p muster --test notch`
