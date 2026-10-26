# handoff

## Overview

The submodule of `src/handoff.rs`, the daemon's restart in place
(`muster reload`, `ARCHITECTURE.md` §4). The running daemon hands every
terminal over, writes what goes with each session (its ring, modes and size,
status machine, hook token) to an unnamed file, keeps that file, the
terminals, its two sockets and its lock open across `exec`, and execs the new
program with `--adopt <descriptor>`. Sessions' programs stay its children and
run on untouched.

This directory holds `fds.rs`, the daemon's only unsafe code (`ARC-12`; the other exception is `muster-notch`'s AppKit code): two
file-descriptor operations no safe interface can express.

## Interface

From the module root `src/handoff.rs`:

- `CarriedSession`, `HandedSession`, `Inherited` (handoff.rs) — what is
  carried across `exec` and what the new program takes over.
- `FORMAT` (handoff.rs) — the handoff file's format version (1).
- `next_program()` (handoff.rs) — the program installed now.
- `can_take_over(program)` (handoff.rs) — asks the new program
  `--can-adopt` before anything is handed over.
- `exec_into(...)` (handoff.rs) — writes the handoff and execs; on failure the
  daemon takes its sessions back.
- `inherit(state)` (handoff.rs) — the new program reads the handoff.
- `ReloadError` (handoff.rs) — why a reload was refused or failed.

In this directory:

- `duplicate(fd)` (fds.rs) — duplicates a descriptor a library owns
  (portable-pty's terminal) into an `OwnedFd`.
- `inherit(fd)` (fds.rs) — takes ownership of a descriptor left open across
  `exec` by the previous version.
- `keep_across_exec`, `close_on_exec` (fds.rs) — clear or set `FD_CLOEXEC` on
  an owned descriptor.

## Invariants

- `fds.rs` holds the daemon's only `unsafe` (`ARC-12`); everything else about
  descriptors goes through safe interfaces on owned values.
- `inherit` is called only on descriptors listed in the handoff.
- Nothing that could lose a session is done before the new program has said it
  can take over (`--can-adopt`).
- If `exec` fails, the daemon takes its sessions back and carries on.

## Side Effects

- Writes an unnamed (unlinked) file holding the handoff.
- Changes descriptors' close-on-exec flags; duplicates descriptors.
- Replaces the process image (`exec`); clients are disconnected and reconnect.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon handoff`
- End to end: `cargo nextest run -p muster --test reload --test upgrade`
