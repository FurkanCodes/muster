# protocol submodules

## Overview

Holds the submodules of [`../protocol.rs`](../protocol.rs), the wire protocol
between the daemon and its clients (`ARCHITECTURE.md` §6). The current version
is **`PROTOCOL_VERSION = 58`** and the oldest accepted is
`MIN_PROTOCOL_VERSION = 58`. Messages are MessagePack with named fields,
inside the frames of [`../frame.rs`](../frame.rs). The files here hold:

- the state a client is sent;
- terminal modes and sizes;
- the plugin messages;
- the GitHub repositories that can be cloned;
- the coding agents that were found.

Every type here is pure data (`ARC-01`). With the `ts` feature, `ts-rs` exports
TypeScript bindings of these types for the desktop app (`ARC-24`).

## Interface

- `Hello`, `Welcome`, `Incompatible`, `negotiate`, `ClientFrame`,
  `ServerFrame`, `Command`, `Reply`, `ErrorBody`, `ErrorCode` and
  `FIRST_PROTOCOL_VERSION` (`../protocol.rs`): the handshake, version
  negotiation and every request and reply. `ClientFrame::since` gives the first
  protocol version whose daemon reads a frame. `Reading` (`Screen` or
  `LastReply`) is what `Command::ReadSession` asks for.
- `Snapshot`, `Delta`, `Health`, `ResourceReport` and `SessionUsage`
  (state.rs, a private module re-exported from `protocol`): the whole state is
  sent once as a `Snapshot`, then each change as a `Delta`. `Health` reports
  how the daemon is doing.
- `TerminalModes`, `MouseTracking`, `MouseEncoding`, `DetachReason`,
  `TermSize` and `PixelSize` (terminal.rs, private and re-exported): what a
  terminal stream and a resize carry.
- `PluginCommand`, `PluginList`, `PluginInfo`, `PluginDetails`,
  `PluginProblem`, `PluginOrigin`, `PluginPreview`, `PluginChange`,
  `PluginAction`, `ChangedBy`, `Required`, `PreviewKind`, `Beside`,
  `HooksTried`, `HookRun`, `HookOutcome` and
  `command_line` (plugins.rs): the plugin registry on the wire.
  `PluginDetails::of(manifest, sha256, found)` builds the details shown for
  approval.
- `HostedRepository`, `repository_name` and `folder_of` (repositories.rs):
  GitHub repositories that can be cloned as a project. The daemon lists them
  through the person's `gh`.
- `AgentFound` (agents.rs): where a coding agent is installed and the version
  it reports, as seen on the daemon's `PATH`.

## Invariants

These are the versioning rules (`ARC-09`), stated in `../protocol.rs`:

- Every change to a protocol type raises `PROTOCOL_VERSION`.
- A new optional field carries `#[serde(default)]`.
- A new variant is sent only to a peer whose agreed version knows it.
  `ClientFrame::since` is checked once in `muster-client`, and a
  `Delta::since` check is added in the daemon when a delta first needs one.
- Removing or renaming anything also raises `MIN_PROTOCOL_VERSION`.
- The handshake (`Hello`, `Welcome`, `Incompatible`, and the error for a frame
  sent before the hello) and the frame header never change shape.
  - Each version's encoding of them is frozen as whole frames in
    `crates/muster-core/tests/frozen/v<N>/`: `hello.msgpack`,
    `welcome.msgpack`, `incompatible.msgpack` and `out-of-order.msgpack`.
    Sets exist for v28 and v30 through v58 (none for v29).
  - `crates/muster-core/tests/frozen_protocol.rs` fails if today's code reads
    any of them differently or encodes them in other bytes.
  - When the version is raised, run
    `cargo test -p muster-core --test frozen_protocol -- --ignored` to write
    the new set, then add its line to `FROZEN`.
  - A committed set is never edited or regenerated.
- Messages that grow from version to version are covered by
  `crates/muster-core/tests/protocol_round_trip.rs` and its sample
  submodules.

## Side Effects

- None. These are serde data types and pure helpers.

## Verification

- Run: `cargo check -p muster-core`, then
  `cargo nextest run -p muster-core --test frozen_protocol --test protocol_round_trip`.
- A protocol version change runs the whole suite
  (`cargo nextest run --workspace`, `ARC-09`), followed by `pnpm bindings` in
  `apps/desktop`.
