# mock

## Overview

The pretend daemon, held in the page. `pnpm dev:mock`, the Playwright suite (`e2e/`) and
the unit tests use it. `MockBridge` implements the same `Bridge` as the real
one. `createBridge()` imports it lazily, only when `__MOCK__` is true, seeds it
with `workingDay()`, and sets it as `window.__muster` so tests and people can
drive it. All state is an in-memory `Scenario` world: projects, workspaces,
sessions, terminal text, spaces, changes, diffs, timelines, tickets, plugins
and preferences.

`open()` sends `link: connecting`, then on a microtask `link: up` and a
`reset` patch with the whole snapshot. Every patch carries `derived`,
recomputed by `derive()`, a TypeScript copy of the `muster-model` rules.
`command()` records each command, honours test hooks, and passes the command
along `handleCommand`'s chain of handlers.

## Interface

- `MockBridge` (bridge.ts) — the pretend daemon. Test hooks include `sent`, `typed`, `attaches`, `sized`, `openedUrls`, `hold`/`release`, `failNext`, `publish`, `output`, `setStatus`, `setModes`, `updateSession`, `removeSession`, `setLink`, `reveal` and `drop`.
- `handleCommand` (commands.ts) — the handler chain: `Ping`, `ListRepositories`, sessions, spaces, settings, undo, rename/stop/remove, changes, by-hand, tickets, flows. It returns `undefined` when no handler knows the command.
- `derive` (model.ts) — `Derived` (needs_you, working, rows, space_rows, space_notes, counts, questions) from the world.
- `World`, `needsAttention`, `standsBeside`, `lastLines`, `workspacesOf`, `listedOf` (model.ts) — the world's shape and the model rules mirrored from Rust. `lastLines` mirrors `link/questions.rs`.
- `workingDay`, `Scenario`, `defaultPreferences`, `ulid` (scenario.ts) — the default seeded world.
- `MockSettings`, `ProjectRecord` (settings.ts) — settings and plugin commands, with test drivers.
- `FlowWorld`, `flowCommand` (flows.ts) — pretend git and file-system flows.
- `ticketCommand`, `agentsOnTickets`, `ticketScenario`, `TicketScenario` (tickets.ts) — a pretend ticket tracker.
- `sessionsCommand` (sessions.ts), `spacesCommand`, `moveTo`, `homeFor`, `folderOf`, `HOME` (spaces.ts), `byHandCommand`, `ByHandWorld` (byHand.ts), `handleUndo` (undo.ts), `NOW` (now.ts), `expandPath`, `folderTree`, `HOME` (folders.ts), and `workingDayPlugins`, `fetchable`, `Fetchable`, `WEBHOOK_FOLDER`, `YOUTRACK_GIT` (plugins.ts) — the other handlers and fixtures.
- `replies.json` — `{what, send, outcome, save}` cases the mock must answer as the real daemon does. They are written from the real daemon by `pnpm replies` and replayed by `replies.test.ts`.

## Invariants

- `ARC-24`: the mock answers as the real daemon does, and `replies.json` is the shared conformance list.
- `ARC-25`: news (unseen) is cleared only while a session is attached, watched and looked at. Questions are given only while the window is seen.
- `ARC-33`: every project is in exactly one space, and a free terminal in the space asked or in none; a Home space is made where none exists.
- `ARC-10`: a new session is stored, and the page told, before its program starts.
- An unknown command is refused with `BridgeFailure("invalid", "The mock daemon does not do X yet")`.
- Replies are `structuredClone`d, so the page never holds the mock's own objects. For commands that create something, the reply comes before the state broadcast.
- The mock is bundled only in mock builds and tests; the real app never carries it.

## Side Effects

- No Tauri calls; everything happens in the page. `window.__muster` is set (in `../index.ts`).
- `queueMicrotask` and `setTimeout(0)` order link-up, buffered broadcasts and simulated program ends.
- In-memory state only: the world, a change clock, terminals, watchers, holds and failures, and the arrays tests read.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/ipc/mock`
- After a daemon reply changes: `pnpm replies` (rewrites `replies.json` from the real daemon), then the tests above.
