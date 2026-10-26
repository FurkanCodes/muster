# trackers/plugin

## Overview

The submodules of `src/trackers/plugin.rs`: one ticket plugin's process and
the bounded streams it talks over. A process is started at the first request,
greeted with `hello`, then asked by the ticket protocol
(`muster_core::ticket_protocol`). It is bounded every way (`ARC-13`,
`ARC-14`). The `Plugins` set, `PluginSpec` and `PluginTickets` that use it are
in `src/trackers/plugin.rs`; see [../README.md](../README.md).

## Interface

- `Process` (process.rs) — one plugin process: `new`, `spec`, `call`, `stop`
  (`log` is test-only).
- `Process::call(call)` (process.rs) — sends one ticket-protocol request and
  waits for its reply, with a deadline.
- `Limits` (process.rs) — the `hello`, `request` and `idle` deadlines. The
  4 MiB reply-line cap is `muster_core::ticket_protocol::MOST_LINE`.
- `MOST_IN_FLIGHT` (process.rs) — at most 8 requests wait on a process.
- `Lines<R>` (streams.rs) — `Lines::new(reader, most)` reads
  newline-delimited replies, each at most `most` bytes long.
- `Log`, `MOST_LOG` (streams.rs) — the plugin's last 64 KiB of log: its stderr
  and the lines its hook runs append; `hide(secrets)` masks tokens, `text`,
  `last_line`.

## Invariants

- At most 8 requests in flight, each with a deadline; a reply line is at most
  4 MiB; the stderr log keeps at most 64 KiB (`ARC-14`).
- A process stops after five minutes unused, and with the daemon (`ARC-13`).
- A crash, or a line that is not a reply, fails every waiting request with the
  reason; the next request starts the process again.
- A request whose deadline passes marks the process hung; it is stopped the
  same way.
- Secrets the plugin was given are hidden from its log before it is shown
  (`PRD-06`).

## Side Effects

- Spawns and kills the plugin's child process.
- Writes JSON lines to its stdin, reads its stdout and stderr.
- Holds a mutex-guarded log shared with `Plugins`.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon trackers::plugin`
- End to end: `cargo nextest run -p muster --test ticket_plugins --test trackers`
