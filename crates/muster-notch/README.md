# muster-notch

## Overview

"Muster's agents in the MacBook's notch while the person looks elsewhere"
(`ARC-22`). A library run by the `muster` program on the main thread: a
daemon client that draws waiting/working counts beside the notch, opens into
a session list on hover, and posts Mac notifications when the program runs as
Muster's app.

Dependencies: `muster-core` and `thiserror` everywhere; on macOS only,
`muster-client`, `tokio`, `objc2` / `objc2-app-kit` / `objc2-foundation` /
`objc2-quartz-core` / `objc2-core-graphics` / `objc2-core-foundation` /
`objc2-user-notifications`,
`block2`, `dispatch2`. On other systems `run` returns `NotchError::NotAMac`.

## Interface

- [src](./src/README.md) — `run`, `NotchError`, `MOVED_FOR`, and the pure modules `board`, `trouble`, `geometry`, `hover`.

## Invariants

- One notch at a time (`notch.lock`); a client of the daemon that never starts it.
- `unsafe` code only in `src/mac.rs` and `src/mac/` (`ARC-12`).
- The pure modules are tested on every OS.

## Side Effects

- macOS UI (AppKit panel, Core Animation, UserNotifications), a lock file in the runtime directory, the daemon socket.

## Verification

- Run: `cargo check -p muster-notch`, then `cargo nextest run -p muster-notch`
- End-to-end: `cargo nextest run -p muster --test notch`
