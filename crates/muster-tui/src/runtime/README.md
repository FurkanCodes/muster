# runtime (submodules of `runtime.rs`)

## Overview

The two helpers of the TUI's event loop in [`../runtime.rs`](../runtime.rs): the
link to the daemon, kept off the UI's loop, and the frame pacing that decides
when the loop draws. `runtime.rs` owns the host terminal, the `tokio::select!`
loop and `perform` (carrying out the app's `Action`s); this directory holds
only what it delegates.

## Interface

- `keep_linked` (link.rs) — task that connects to the daemon (starting it if
  nothing answers), forwards its frames to the loop and reconnects whenever the
  connection is lost.
- `LinkEvent` (link.rs) — what the link reports: `Connected { sender, welcome }`,
  `Frame(Box<ServerFrame>)`, `Lost(reason)`.
- `Outgoing` (link.rs) — handle for frames to the daemon; `send` hands a
  `ClientFrame` to the connection's own writer task, `false` once it is gone.
- `LINK_EVENTS` (link.rs) — bound (64) of the queue from the link to the loop.
- `START_WAIT`, `START_AGAIN_AFTER` (link.rs, re-exported by `runtime.rs`) —
  5 s to reach a daemon the UI started; at most one start per 10 s.
- `Draws` (drawing.rs) — frame pacing: `woke`, `actions`, `input`,
  `expect_echo`, `output`, `due`, `drawn`, `wake`. A burst of output shares one
  frame (`FRAME`, 16 ms); a key press and the first echo of its input draw at once.
- `link_tests.rs` — unit tests of the link (not part of the interface).

## Invariants

- The loop never waits on the daemon: frames for it go out through an
  unbounded channel to a writer task of their own (`ARC-14`).
- Frames from the daemon wait in a bounded queue (`LINK_EVENTS`); a loop that
  falls behind stops the link reading, and the daemon treats this client as
  any slow one (terminal output dropped and resumed from the ring). Nothing
  that must arrive is dropped by the client (`ARC-14`).
- Spawned tasks are owned by an `AbortOnDropHandle` and end with the
  connection or the UI (`ARC-13`).
- Retries: every 500 ms for the local socket; for another machine (`connect`
  set, through ssh) doubling up to 15 s, reset only after a link that lasted
  30 s (`ARC-30`).
- Synchronized output (`FrameHold`) holds a draw while the shown session is
  mid frame, for at most `FRAME_WAIT` (150 ms), so a frame that never ends
  cannot stop drawing (`ARC-13`); there is no periodic idle draw.
- Time comes from `runtime.rs`'s `monotonic()`; nothing here reads the clock
  for the app (`ARC-03`).

## Side Effects

- Unix-socket connections to the daemon through `muster-client`
  (`Connection::connect`, client name `muster-tui <version>`), or through the
  `Options::connect` connector.
- May start the daemon via `Options::start_daemon`.
- Spawns tokio tasks (the link, the writer) and sleeps between retries.
- `Draws` has none: pure bookkeeping over `Instant`s.

## Verification

- Run: `cargo check -p muster-tui`, then `cargo nextest run -p muster-tui runtime`
- End to end: `cargo nextest run -p muster --test 'tui*'`
