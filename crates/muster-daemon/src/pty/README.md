# pty

## Overview

The submodules of `src/pty.rs`, hands-on sessions' terminals
(`ARCHITECTURE.md` §5). Each session's terminal is owned by one task, its
actor. A reader thread feeds it output, which it appends to the ring and
broadcasts in one step, so an attaching client gets the replay and the live
stream with no gap and no overlap. It also draws the output on a kept screen,
answers the questions programs ask their terminal, and scans for progress,
title and agent-end reports. Keystrokes go straight to the actor, never
through the Supervisor.

An actor can hand its terminal over for the restart in place (`handoff`): it
stops its reader between reads and gives up the terminal with its ring,
screen, input modes and size; the next daemon adopts it.

## Interface

From the module root `src/pty.rs`:

- `spawn(...)`, `adopt(...)` (pty.rs) — start a session's program on a new
  terminal, or adopt one handed over.
- `PtyHandle` (pty.rs) — the actor's handle: `attach(from_seq)`, `input`,
  `resize`, `resize_now`, `focus`, `focus_now`, `stop`, `resume_if_suspended`,
  `pid`, `foreground`, `text`, `hand_over`.
- `SpawnSpec` (pty.rs) — program, arguments, environment, size.
- `PtyEvent`, `Chunk`, `Attachment` (pty.rs) — what the actor reports; a
  replay plus live stream for one client.
- `Carried`, `Handed` (pty.rs) — a terminal and its state across `exec`.
- `Colours` (pty.rs) — watch of the person's terminal colours.

In this directory:

- `Ring`, `Replay`, `RING_CAPACITY` (ring.rs) — the newest 1 MiB of output,
  numbered by byte.
- `Screen` (screen.rs) — the screen the output draws, sent whole to a client
  starting over; answers cursor-position queries.
- `Report`, `OscScanner` (osc.rs) — OSC 9;4 progress, OSC 0/2 titles and
  `muster exec` agent-end markers (`ARC-28`).
- queries.rs — answers keyboard-protocol, device-attribute, version, DECRQM and
  pixel-size queries; tracks focus, kitty keyboard, bracketed paste, cursor,
  mouse and alternate-screen modes.
- `Colour`, `questions`, `answer` (colours.rs) — OSC 10/11/4 colour queries,
  answered from the colours a UI reported.
- `Input`, `Typed`, `queue` (input.rs) — bounded typed/pasted input on its way
  to the writer thread.
- props.rs — property tests for the scanners.

## Invariants

- Appending to the ring and broadcasting happen in one step, so replay and
  live stream never gap or overlap.
- No byte is taken from the terminal that the ring does not hold, including
  across a hand-over.
- Input is never dropped while the program reads it, never waited on by the
  actor, and bounded by bytes (`ARC-14`).
- Scanners never panic on any bytes and give the same result however output is
  split into reads; only changes are reported.
- A colour no UI has reported is not answered.
- The actor outlives the process, so an exited session's last output can be
  read until the Supervisor releases it.

## Side Effects

- Spawns child processes on pseudo-terminals (portable-pty), as session
  leaders with the terminal as controlling terminal.
- Runs three threads per terminal: a reader, a writer and `muster-pty-wait`,
  which waits on the child.
- Writes query answers back into the terminal.
- Sends signals to the child (stop, resume a suspended one).

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon pty`
- End to end: `cargo nextest run -p muster --test terminal --test sessions --test detach`
