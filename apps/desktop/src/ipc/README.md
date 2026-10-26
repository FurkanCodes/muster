# ipc

## Overview

The one door between the webview and the daemon. Pages talk only to a
`Bridge` and never call Tauri's `invoke` themselves. `TauriBridge` is the real
bridge over Tauri commands and channels. In mock builds, the pretend daemon in
[`./mock/`](./mock/README.md) stands in for it. State arrives in order on one
`Channel<StateMessage>`. Each attached terminal gets its own channel of binary
frames.

`./generated` holds the TypeScript types generated from Rust by ts-rs: the
daemon's protocol (`muster-core`) and the app's contract
(`src-tauri/src/contract.rs`). They are written by `pnpm bindings` and never
edited by hand.

## Interface

- `Bridge` (bridge.ts) — `open`, `tryNow`, `showDaemonLog`, `command`, `openUrl`, `attach`, `detach`, `input`, `resize`, `watch`, `looking`, `colours`, `listFolders`, `pickFolder`, `onFolderDrop`.
- `BridgeFailure` (bridge.ts) — kind `unreachable | refused | invalid` with an optional `code`, mirroring Rust's `BridgeError`.
- `createBridge` (index.ts) — `TauriBridge` (after `keepIn`, so window state is restored before drawing), or a lazily imported `MockBridge(workingDay())` when `__MOCK__` is set.
- `TauriBridge` (tauri.ts) — the real bridge, plus `hear`, `restartInPlace`, `openNotificationSettings`, `editSettingsFile`, `kept` and `keep`.
- `decodeFrame`, `encodeFrame`, `TermFrame` (frames.ts) — the binary terminal frame `[kind u8][seq u64 LE][payload]`.
- `FRAME_OUTPUT`, `FRAME_REPLAY`, `FRAME_REPLAY_RESET`, `FRAME_SCREEN`, `FRAME_EVENT` (frames.ts) — kinds 0, 1, 2, 3, 9 (an event's payload is a JSON `TermEvent`).
- `ask`, `replyAs`, `ReplyKind`, `ReplyOf` (ask.ts) — send a command and narrow its `Reply` to the expected variant, or throw.
- `useAsked`, `Asked` (useAsked.ts) — ask once per key: `asking | answered | failed`; nothing is asked while `ask` is null.

## Invariants

- Only this folder calls Tauri. Commands map one to one to the Rust commands in [`src-tauri/src`](../../src-tauri/src/README.md).
- Terminal output stays bytes end to end and never goes through JSON. The frame layout matches `contract.rs`.
- The mock is bundled only when `__MOCK__` is true.
- `openUrl` opens a ticket's address only when asked to explicitly.
- `useAsked` drops answers to keys that are no longer current.
- Types come from `./generated`, never written by hand.

## Side Effects

- Tauri `invoke`: `link_open`, `try_now`, `show_daemon_log`, `command`, `open_ticket_url`, `hear`, `restart_in_place`, `open_notification_settings`, `edit_settings_file`, `kept`, `keep`, `attach`, `detach`, `input`, `resize`, `watch`, `looking`, `colours`, `list_folders`, `pick_folder`.
- Tauri channels: one for state, and one per attached terminal. `getCurrentWebview().onDragDropEvent` forwards dropped paths.
- `createBridge` restores kept window state (errors swallowed), and sets `window.__muster` in mock builds.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/ipc`
- After changing a Rust type the webview uses: `pnpm bindings`.
