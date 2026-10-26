# lib

## Overview

Small pure helpers the webview shares: how an agent is named and drawn, how a
session's status reads, and how numbers, times, paths and text are shown. No
React, no store, no IPC — only functions over the generated protocol types
(`../ipc/generated`).

## Interface

- `OWN` (agents.ts) — Muster's own coding agents in the order offered (`muster_core::harness::OwnAgent`).
- `Agent`, `agentOf(harness)` (agents.ts) — a harness id's name, logo, mark and `mu-logo--*` tone; `plugin:<plugin>/<name>` harnesses and unknown ones get a plain mark.
- `cn(...names)` (cn.ts) — joins class names, dropping empty or false ones.
- `ago(millis)` (format.ts) — relative time: `32s`, `4m`, `3h`, `2d`.
- `spend(tokens, costMicros)` (format.ts) — `182k tokens`, `$0.42, 40k tokens`.
- `removed(count)`, `splitPath(path)` (format.ts) — true minus sign; a path's folder and name.
- `StatusKind`, `statusKind(session)` (status.ts) — working / waiting / done / error / ended / fresh.
- `doing(session)` (status.ts) — a session's second line: its activity, else its status in words, noting when an agent started it (`PRD-01`).
- `oneLine(text)` (text.ts) — line breaks and tabs as spaces, other control characters dropped (mirrors `muster_core::text::one_line`).
- `errorText(error)` (text.ts) — an error's message for the person.

## Invariants

- Every function is pure and synchronous; nothing here reads the store or the daemon.
- `oneLine` must keep matching `muster_core::text::one_line`, and `OWN` must keep matching `muster_core::harness::OwnAgent`.
- `statusKind` and `doing` switch exhaustively over the generated `Session` status; a new state in the protocol is a type error here.

## Side Effects

- None. `agents.ts` imports the agents' SVG logos from `../assets/agents`.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/lib`
