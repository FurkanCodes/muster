# server

## Overview

The submodule of `src/server.rs`, the socket server: one reader loop and one
writer task per connection, so replies and pushed frames never interleave
mid-frame (`ARCHITECTURE.md` §6). `outbox.rs` decides what the writer sends
and in which order.

Two kinds of frame leave for a client. Replies, the state and its deltas must
all arrive, in order: they wait in a bounded queue, and a sender waits while it
is full (a delta pump that falls behind gets a fresh snapshot instead). A
session's terminal output need not: the session's ring holds it, numbered by
byte, so each attached session's stream frames wait in a lane of their own,
capped at `LANE_BUDGET` bytes. A client past that is behind: its lane is
dropped and the stream resumes later from the first byte dropped, as a replay
from the ring or a full `reset` (`ARC-14`).

## Interface

From the module root `src/server.rs`:

- `serve(...)` (server.rs) — accepts connections and runs each one's reader
  and writer.
- `pump_state(supervisor, outgoing)` (server.rs) — sends the state snapshot and
  its deltas to one connection.

In this directory:

- `Outbox` (outbox.rs) — one connection's outgoing frames: `new(budget)`
  (returns `Arc<Outbox>`), `push(session, frame)`, `is_behind`, `caught_up`,
  `clear`; `Gone` when the connection closed.
- `write_frames` (outbox.rs, `pub(crate)`) — the per-connection writer task
  `server.rs` spawns.
- `Lanes` (outbox.rs) — per-session stream lanes: `push`, `pop`, `catch_up`,
  `is_behind`, `resume`, `clear`.
- `Pushed` (outbox.rs) — whether a frame was queued or the lane is behind.
- `Resume` (outbox.rs) — the byte a behind stream resumes from.
- `LANE_BUDGET` (outbox.rs) — 256 KiB of output per lane (beyond the one
  replay a client resumes with).

## Invariants

- Frames never interleave mid-frame: one writer per connection.
- Replies, state and deltas are never dropped and keep their order.
- No terminal byte reaches a client twice, and a gap is always marked `reset`.
- A stream that ends while its client is behind ends after an empty replay
  marked `reset` from the end of what was dropped.
- The writer takes queued frames first, then each session's lanes in turn, one
  frame each.

## Side Effects

- Reads and writes the daemon's Unix sockets.
- Sends requests to the Supervisor and receives its pushes over channels.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon server`
- End to end: `cargo nextest run -p muster --test lifecycle --test detach --test slow_client`
