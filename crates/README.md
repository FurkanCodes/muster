# crates

## Overview

The Rust workspace behind Muster: eight crates around one rule, *the daemon owns everything and clients only draw it* (`ARC-02`). The pure crates (`muster-core`, `muster-state`, `muster-model`) do no IO (`ARC-01`); the daemon reaches Docker, git hosts, ticket sources, harnesses, PTYs and the clock only through traits (`ARC-03`). Clients (TUI, CLI, notch, and the desktop app in [`apps/desktop`](../apps/README.md)) talk to the daemon through `muster-client` over the protocol defined in `muster-core`.

Dependency direction: `muster-core` ← `muster-state`, `muster-model`, `muster-client` ← `muster-daemon`, `muster-tui`, `muster-notch` ← `muster` (the one binary).

## Subdirectories & Boundaries

- muster-core: [./muster-core/README.md](./muster-core/README.md) - IO-free shared library: typed ULID ids, entities and states, the daemon/client wire protocol (`PROTOCOL_VERSION` 58, frozen handshake samples per version, `ARC-09`), length-prefixed MessagePack framing, the ticket-plugin JSON-RPC protocol, strict settings and plugin-manifest readers (`ARC-05`, `PRD-15`), the keymap and layout, git-output parsers and sanitisers for outside text (`PRD-06`, `PRD-07`). Optional `ts` feature exports TypeScript bindings for the desktop app (`ARC-24`).
- muster-state: [./muster-state/README.md](./muster-state/README.md) - Pure state machines with an injected clock: the session status machine (hook events, OSC 9;4 progress, exits → status, unseen mark and effects; carried across `muster reload`) and the run machine (ticket → checks → review → PR with attempt and time budgets, `PRD-02`, `PRD-12`).
- muster-model: [./muster-model/README.md](./muster-model/README.md) - A client's copy of daemon state built from Snapshot + Deltas: Needs-you order, list rows, spaces views and counts, shared by the TUI and the desktop app so they answer alike (`ARC-24`, `ARC-31`). No IO.
- muster-client: [./muster-client/README.md](./muster-client/README.md) - The async framed transport and greeted `Connection` (handshake, subscribe, commands, terminal attach) over the daemon's Unix socket or a carrier stream such as ssh; refuses to send frames newer than the agreed protocol (`ARC-09`).
- muster-daemon: [./muster-daemon/README.md](./muster-daemon/README.md) - The daemon library: a single Supervisor task owning all state (`ARC-02`), SQLite store written before every broadcast (`ARC-10`), one PTY actor per session with a byte-numbered ring, harness launch/hooks without editing the person's config (`PRD-13`), ticket sources and git hosts behind traits, plugins with approved-hash checks (`PRD-15`), timelines for undo (`PRD-14`), spend readers, and restart in place (its only `unsafe`, `ARC-12`).
- muster-tui: [./muster-tui/README.md](./muster-tui/README.md) - The terminal UI: a pure `App` state machine, pure ratatui drawing, and a runtime holding all IO (host terminal, daemon link, clock); per-session vt100 emulation and key/mouse/paste encoding.
- muster-notch: [./muster-notch/README.md](./muster-notch/README.md) - The MacBook notch status and Mac notifications (`ARC-22`): pure board/geometry/hover modules tested everywhere, AppKit drawing on macOS only.
- muster: [./muster/README.md](./muster/README.md) - The one shipped binary (clap CLI): opens the TUI, starts the daemon detached (`PRD-09`), the shell verbs that drive agents and terminals, plugins, `muster ssh` / `--on` over the person's own ssh (`ARC-27`), verified `muster upgrade` (`ARC-21`); `tests/` holds the end-to-end suite against the real daemon binary.
