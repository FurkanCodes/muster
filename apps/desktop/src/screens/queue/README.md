# queue

## Overview

The *Needs you* screen. It lists the sessions waiting on the person, grouped by
project. The order is always the model's `derived.needs_you`: sessions asking
something come first, then finished ones not yet seen. The webview never
sorts it. Running agents are not listed, only counted in the empty state.
Sessions that stand beside a chat (shells, plugin panels) are left out. The
keys mirror the terminal UI's: ↑↓/k j, ⏎, `l` (last alarm), `z` (put back),
`R`, `x`, `b`.

## Interface

- `QueueScreen` (QueueScreen.tsx) — the list screen; `active` gates its keys and hints.
- `NeedsYouPage` (QueueScreen.tsx) — the screen as a standalone page, always active.
- `Section`, `sectionOf` (queue.ts) — `asking | news | working`, from a session's status and unseen flag.
- `queueIds` (queue.ts) — `derived.needs_you` without the sessions that stand beside a chat, in the model's order.
- `holdPlace` (queue.ts) — the shown ids, keeping the selected one in place after it leaves the queue until another is selected.
- `projectOfSession`, `ProjectGroup`, `byProject` (queue.ts) — a session's project; ids grouped by project in first-seen order, so the most urgent project leads.
- `installQueueHistory` (history.ts) — subscribes to the daemon store for the window's life; returns an unsubscribe.
- `lastNeeded` (history.ts) — the session that most recently came to need the person (the `l` key).
- `changedInSight`, `stamp` (history.ts) — whether a status changed while watched (so the row flashes once); its `status@since` key.
- `QueueEntry`, `optionId` (QueueEntry.tsx) — one row (asking, finished or working) and its DOM id for `aria-activedescendant`.

## Invariants

- `ARC-24`: the queue's order is the model's, never the webview's.
- The selected session keeps its place while it still exists. It is dropped once another is selected or the keys come back to the list. Then the selection goes to the first in the queue if the selected one no longer waits.
- A snapshot at start or after a reconnect is no news: it neither flashes nor sets `lastNeeded`.
- A row flashes once per status change (its React key is `stamp(session)` while flashing).
- Put back is refused while a chat in the workspace is running, or when no turn is kept for it.

## Side Effects

- No daemon IPC of its own. Rename, close and the pull request in the browser go through `useShownSessionActions()` in [`src/session`](../../session/README.md).
- Store: `select`, `show("all")`, `showPane("terminal")`, `showTerminal`, `keysTo`, `setHints` (cleared when inactive or unmounted), `showTurn(workspace, turn, "put-back")`, `tell`.
- `history.ts`: a `daemon.store.subscribe` and module-level `WeakMap` state keyed by the daemon.
- Keys: `useKeys`, `useActionKeys("needs_you", …)`, `useBorrowedKeys("needs_you", put_back, pull_request_in_browser)`.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/screens/queue`
