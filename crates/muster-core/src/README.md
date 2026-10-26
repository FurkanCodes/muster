# muster-core (src)

## Overview

These are the types every Muster crate shares: the daemon, the client, the TUI,
the notch helper and the desktop app's Rust side. They are:

- typed ids and the entities the daemon stores;
- session, run and ticket states;
- the wire protocol and its framing;
- the strict readers of `muster.toml`, `settings.toml` and plugin manifests;
- the keymap and the panel layout;
- small pure helpers over the text git prints and over text from outside
  Muster.

[`lib.rs`](./lib.rs) declares 39 public modules, one file each, and re-exports
the most used types at the crate root. The crate does no IO (`ARC-01`) and
forbids unsafe code (`#![forbid(unsafe_code)]`, `ARC-12`).

`./layout/` and `./ticket_protocol/` hold only the unit tests of `layout.rs`
and `ticket_protocol.rs`, so they have no README.

## Interface

Re-exported at the root (lib.rs):

- `ProjectId`, `WorkspaceId`, `SessionId`, `RunId`, `SpaceId` and
  `ParseIdError` (ids.rs): ULIDs printed with a short prefix (`ses_01J…`).
  Each entity has its own type, so ids cannot be mixed up.
- `Project`, `Workspace`, `WorkspaceKind`, `Session`, `Issue`,
  `IssueComment` and `TicketBrief` (entities.rs): the things the daemon stores
  and clients draw.
- `SessionStatus`, `WaitReason`, `Activity`, `AgentNow` and `Spend`
  (session.rs); `Run`, `RunStage`, `RunTransition`, `Ticket` and `PullRequest`
  (run.rs).
- `Changes`, `ChangeSummary`, `FileChange`, `FileDiff`, `ChangeKind` and
  `ChangePart` (changes.rs): parse what git prints about a workspace's
  changes.
- `PullRequestStatus`, `PullRequestState`, `Checks` and `Review` (pulls.rs);
  `TicketKey` (tickets.rs); `Space` and `Member` (spaces.rs); `Timestamp`
  (time.rs); `Paths` (paths.rs).

Modules (one file each unless linked):

- Wire:
  - `protocol`, with [`./protocol/README.md`](./protocol/README.md): daemon to
    client messages, `PROTOCOL_VERSION` 58 (`ARC-09`).
  - `frame`: `[u32 LE length][MessagePack]` with `encode`, `FrameDecoder` and
    `MAX_FRAME_LEN`.
  - `ticket_protocol`: JSON-RPC 2.0 to ticket plugins, one message a line
    (`PROTOCOL` 1). Its `request_line`, `read_reply`, `listed` and `viewed`
    check every answer (`PRD-07`).
- Settings:
  - `config`, with [`./config/README.md`](./config/README.md): `ProjectConfig`
    and `MusterConfig`.
  - `settings`: `ProjectSettings`, `Preferences` and `SettingChange` as the
    protocol carries them (`ARC-05`).
  - `keys`, with [`./keys/README.md`](./keys/README.md): `Action`, `Chord` and
    `Keymap`.
  - `layout`: the UI's panel tree (`Layout`, `Node`, `Split` and `Panel`).
  - `telling`: notification choices and `MAC_SOUNDS`.
  - `colours`: terminal colours for OSC 4, 10 and 11.
- Plugins: `plugins`, with [`./plugins/README.md`](./plugins/README.md):
  manifests, hooks, events, placeholders and the registry (`PRD-15`).
- Sessions and agents:
  - `harness`: `SessionKind`, read from a harness string.
  - `front`: `identify`, which tells which program or agent is in front
    (`ARC-28`).
  - `ask`: how a coding agent answers one question.
  - `names`: chat names.
  - `delegation`: agents starting agents (`PRD-01`).
  - `explain`: `muster explain`.
  - `news`: what needs the person.
- Git work:
  - `changes`;
  - `clashes`: overlapping and conflicting files across workspaces
    (`PRD-14`);
  - `timeline`: turn entries and undo plans;
  - `tidy`;
  - `stacks`;
  - `remote`: the host and path in a git remote;
  - `pulls`;
  - `tickets`: `ticket_branch`, which names branches
    `muster/issue-<key>-<title>`.
- Machines: `machines` (`machines.toml`, which holds ssh targets only and
  never keys).
- Spaces: `spaces`.
- Safety of outside text:
  - `credentials::hide`: hides secrets (`PRD-06`).
  - `text`: `one_line` and `multi_line` strip control characters.
  - `html::to_text`.
- Small helpers:
  - `hex`;
  - `num`: checked rounding;
  - `version`: `Version` compares number by number;
  - `time`: `Timestamp` and RFC 3339;
  - `paths`: `Paths::resolve(env, uid)` is a pure function of a passed-in
    environment.

## Invariants

- No IO (`ARC-01`). No module touches files, sockets, processes or
  environment variables.
  - The clock is read by the daemon (`ARC-03`). `Timestamp` only carries a
    value. The one exception is a new id: each id type's `new()` calls
    `Ulid::generate()`, which reads the clock and randomness (ids.rs).
  - `Paths::resolve` takes an environment lookup function instead of reading
    `std::env`.
  - The only `std::fs` and `std::env` calls are in tests behind
    `cfg(all(test, feature = "ts"))`, which export bindings.
- No unsafe code (`#![forbid(unsafe_code)]`). The audited `unsafe` lives in
  the daemon's `handoff::fds` and the notch's `mac` (`ARC-12`).
- Strict reading (`ARC-05`): every settings and manifest parser rejects an
  unknown key and names it.
- Text from outside Muster (tickets, git, plugin replies) is cleaned of
  control characters and credentials before anything shows it or hands it to
  an agent (`PRD-06`, `PRD-07`).
- No source file over 1,000 lines and no struct over 40 fields (`ARC-08`). The
  largest file, `layout.rs`, has 860 lines.

## Side Effects

- None at run time.
- Behind the `ts` feature, test runs write TypeScript bindings and
  `keys.json`/`MacSounds.ts` into `TS_RS_EXPORT_DIR` (via
  `scripts/bindings.sh` and `pnpm bindings`).

## Verification

- Run: `cargo check -p muster-core`, then `cargo nextest run -p muster-core`.
- Then run the crates and end-to-end files that use what changed (see the
  table in `CLAUDE.md`).
