# changes

## Overview

The Changes board for one workspace (`PRODUCT.md` §6). It is lazy-loaded into the
session pane in place of the terminal. `ChangesView` waits until the daemon
store knows the workspace, and remounts fresh on each `ui.changesTurn.request`.
It has three views, *uncommitted*, *branch* (commits and files) and *turns*,
switched with a segmented control or ⇥/⇧⇥. The keys follow the terminal UI:
arrows/jkl/h, PgUp/PgDn, Home/End, ⏎, and ⎋ back to the terminal. The view is
read-only ("Muster only asks git"). Putting a turn back is the one change it
makes.

## Interface

- `ChangesView` (ChangesView.tsx, default export) — the board for a `WorkspaceId`: selection, open diff, list or diff keys, hints, and the keymap actions `close_changes`, `leave_session`, `changes_again`, `put_back`, `changes_help`, `fold_list`.
- `ChangesList`, `View` (ChangesList.tsx) — the list column for each view, with loading, refused and empty states.
- `FileList`, `inSight`, `ROW` (FileList.tsx) — virtualized listbox of file changes (26px rows, overscan 8) that keeps the selection in sight.
- `DiffPanel` (DiffPanel.tsx) — asks `FileDiff` or `EntryDiff` and draws a `DiffView`.
- `PutBack`, `beforeWhat`, `branchWords` (PutBack.tsx) — asks for the `UndoPlan`, confirms, then sends `Undo`.
- `ClashBanners` (ClashBanners.tsx) — a conflict or overlap banner per other workspace touching the same files, with 1..N keys to open that workspace's changes.
- `useWorkspaceChanges` (useWorkspaceChanges.ts) — the listing and timeline answers, kept in step with the store's summary and newest entry.
- `useReply`, `Answer<T>` (useReply.ts) — a visibility-gated, latest-wins `bridge.command` hook keyed by command and revision.
- `entryLabel`, `turnOf`, `summaryOf`, `sameSummary`, `commonFolder`, `placeName`, `clashFiles`, `clashWords`, `wouldConflict` (labels.ts) — pure helpers, worded as the TUI words them.

## Invariants

- `ARC-25`: only file rows in sight are drawn, and every row is one height, so nothing is measured. Nothing is asked while the window is hidden. A window seen again asks only what it has no answer to. The latest request wins, and an answer lost on the way is asked again.
- `PRD-14`: putting back is files-only by default (⏎/`y`). Moving the branch as well must be chosen (`b`) and is never the default. The branch option is hidden when the plan refuses it.
- Nothing polls. The listing is asked again only when the store's summary differs from it, the timeline only when its newest entry moves, and both on `r`.
- Muster keeps each workspace's newest 100 timeline entries.
- Hints whose key is unbound (`none`) are not shown.

## Side Effects

- IPC (`bridge.command`): `WorkspaceChanges`, `Timeline`, `FileDiff`, `EntryDiff`. IPC (`ask`): `UndoPlan` → `UndoPlan`, `Undo` → `Undone`.
- After an undo, the new entry is selected, the diff closes, and the listing and timeline are asked again.
- Store: `showPane`, `setHints` (cleared on unmount), `tell`, `openPalette(true, "changes")`, `foldSidebar`; `ClashBanners` calls `select("all", id)`, `show("all")` and `showPane("changes")`.
- DOM: list focus, `scrollTop` writes, a `ResizeObserver` on the list box.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/session/changes`
