# muster-client

## Overview

Library crate for talking to the Muster daemon over its Unix socket (or any
stream that carries its frames, such as `ssh` to another machine). It wraps
`muster_core::frame` and `muster_core::protocol` with async IO: a framed
transport and a handshaking client connection. Every client (TUI, CLI,
desktop link, end-to-end tests) talks through it, and the daemon uses its
transport for the server side.

Dependencies: `muster-core`, `serde`, `thiserror`, `tokio`.

## Interface

- [src](./src/README.md) — `Connection`, `FrameSender`, `FrameReceiver`, `FramedRead`, `FramedWrite`, `ClientError`, `TransportError`, `daemon_pid`, `daemon_identity`.

## Invariants

- No unsafe code (`#![forbid(unsafe_code)]`, `ARC-12`).
- Every frame after the handshake is checked against the agreed protocol version before sending (`ARC-09`); the opening `Hello` is written as is.

## Side Effects

- Unix-socket and stream IO through tokio; no files, no processes of its own (a carrier process is only held, not spawned).

## Verification

- Run: `cargo check -p muster-client`, then `cargo nextest run -p muster-client`
- End-to-end: `cargo nextest run -p muster --test lifecycle --test detach --test slow_client`
