# muster-client/src

## Overview

The client side of the daemon's wire protocol: an async framed transport over
any byte stream, and a greeted `Connection` that performs the hello handshake,
subscribes to state, sends commands and streams session terminals. Used by the
TUI, the `muster` command, the daemon (for its side of the transport) and the
end-to-end tests. Frame encoding itself lives in `muster_core::frame`; this
crate is its IO half.

## Interface

- `FramedRead<R>` (transport.rs) — reads whole frames from an `AsyncRead`; `next()` returns `None` on a clean EOF between frames.
- `FramedWrite<W>` (transport.rs) — writes and flushes whole frames (`send`, `send_bytes` for malformed-frame tests, `shutdown`).
- `TransportError` (transport.rs) — `Io`, `Frame` (any `muster_core::frame::FrameError`: decode, too large, encode), `Truncated` (EOF mid-frame).
- `Connection` (connection.rs) — an open, greeted connection: `connect(socket, client)` over the Unix socket, `over(reader, writer, client)` over any stream (e.g. `ssh` stdio), `holding(..)` keeps a carrier process alive, `welcome()` gives the daemon's `Welcome`.
- `Connection::subscribe` / `command` / `next_event` (connection.rs) — first snapshot; request/reply matched by `req_id`; pushed snapshots, deltas and terminal frames.
- `Connection::attach` / `attach_unwatched` / `detach` / `watch` / `looking` / `input` / `resize` / `resize_in_pixels` / `colours` / `keys` (connection.rs) — terminal streaming and input frames.
- `Connection::into_split` (connection.rs) — splits into `FrameSender` + `FrameReceiver` + `Welcome` for interactive clients.
- `FrameSender` (connection.rs) — `send`, `send_all(mpsc receiver)`; keeps the held carrier alive.
- `FrameReceiver` (connection.rs) — `next`, `forward(take)` until `take` returns `false` or the connection ends.
- `daemon_pid` / `daemon_identity` (connection.rs) — the pid (and version) of whatever daemon answers on a socket, whichever protocol it speaks.
- `ClientError` (connection.rs) — `Unreachable`, `Transport`, `Incompatible`, `Daemon`, `Closed`, `Unexpected`, `Carrier`, `DaemonTooOld`, `Silent`.
- `ANSWER_WITHIN` (connection.rs) — 30 s limit for the hello answer and the first snapshot.

## Invariants

- No unsafe code: `#![forbid(unsafe_code)]` (`ARC-12`).
- Every frame after the handshake goes through `sendable` (the opening `Hello`, in `over` and `daemon_identity`, is written directly): a frame whose `ClientFrame::since()` is newer than the agreed protocol version is refused with `DaemonTooOld` and never sent (`ARC-09`).
- The hello answer and the first snapshot must arrive within `ANSWER_WITHIN`, else `ClientError::Silent`; `command` replies have no time limit.
- Frames pushed while waiting for a reply (snapshot, delta, replay, output, modes, detached) are queued and delivered in order by `next_event` (or first by the split receiver); any other unexpected frame is an error.
- A server `Error` frame always surfaces as `ClientError::Daemon`.
- Command `req_id`s start at 1 and increase per connection.

## Side Effects

- Connects to the daemon's Unix socket (`tokio::net::UnixStream`).
- Reads and writes the socket or a carrier stream; each write is flushed.
- Uses the tokio timer for `ANSWER_WITHIN`.

## Verification

- Run: `cargo check -p muster-client`, then `cargo nextest run -p muster-client`
- End-to-end: `cargo nextest run -p muster --test lifecycle --test detach --test slow_client` (a framing or handshake change runs the whole suite)
