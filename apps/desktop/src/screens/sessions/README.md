# sessions

## Overview

The desktop app's All sessions list in the sidebar, and the project page shown beside
it. `tree.ts` is pure logic. It turns the model's `derived.rows` (or
`derived.space_rows[space]` when the window has a space) into lines of four kinds:
space, project, workspace and session. `useTree` joins this to the Zustand stores,
`AllSessions` binds the keys and hints and renders the virtualized `SessionTree`,
and `ProjectPage` shows a project's composer and the places where its work is.

## Interface

- `AllSessions` (AllSessions.tsx, default export) — the All list: TUI-style keys (arrows, ⏎, fold, shell, rename, close, checks/review/pull-request), hints, and the tree; prop `active`.
- `ProjectPage` (ProjectPage.tsx) — a project's header and menu, the composer that starts work (new worktree or an existing place, agent choice, prompt, ⌘⏎) and a "Where its work is" list of its checkout and worktrees.
- `TreeRow`, `allRows`, `listedRows`, `shownRows` (tree.ts) — one line; all lines from `DaemonState`; the lines the window's space lists; the lines left once folds are applied.
- `foldOf`, `toggled`, `selectionAfterFold`, `unfoldedTo`, `keptFolds` (tree.ts) — pure fold logic.
- `sessionsOn`, `mostUrgent` (tree.ts) — the sessions under a line; the first needing you, else the first running.
- `useTree`, `Tree` (useTree.ts) — `{ rows, folded, selected, fold(id), pick(id) }`. Rows are compared as strings with `useShallow`, so the list re-renders only when a line is added, removed or moved.
- `readFolds`, `writeFolds` (folds.ts) — folded project ids, kept through `state/kept.ts` under `muster.sessions.folds` (version 1): in the app's `window.json` once `keepIn` has run, in local storage in mock builds.
- `movedProjects`, `useProjectDrop` (projectOrder.ts) — reorder projects by dragging a project line.
- `estimate`, `placesOf`, `lineAt`, `drawn`, `inSight`, `OVERSCAN`, `UNMEASURED` (sight.ts) — virtualization maths.
- `SessionTree` (SessionTree.tsx) — virtualized ARIA tree using `aria-activedescendant`.
- `ProjectLine`, `SpaceLine`, `WorktreeLine`, `SessionLine`, `ShellLine`, `BesideLine`, `rowId` (TreeRows.tsx); `RowDetail` (RowDetail.tsx) — line renderers and a line's detail.

## Invariants

- `ARC-33`: each space's heading is drawn with its projects, and its terminals in no project, under it. Nothing here orders lines: the order is the model's `derived.rows`.
- `ARC-25`: only lines in sight, plus `OVERSCAN` (320px) either side, are drawn between spacers.
- Folds are the window's own; the daemon is never told.
- The selection (`ui.selected.all`) rests only on a drawn line. Folding its parent moves it to the parent; selecting a hidden line unfolds what hides it.
- A worktree has a line of its own only while no session is in it, unless its project has work outside its checkout (then its places are labelled by branch).
- A plugin panel with a chat in its workspace has no line; shells always have one.
- A container workspace is not offered as a place to start work. A plain folder (no branch) has no worktrees, so New worktree is hidden.
- Project order follows `PRODUCT.md` §6a: a dragged project lands after the target when it moves down, and before it when it moves up.

## Side Effects

- IPC: `ChangePreference { ProjectOrder }` on a project drop. Through flows: `makeWorktree`, `startSession`, `openStarted`, `newSession`, `openShell`, and the by-hand checks, review and pull-request actions.
- Kept window state via `state/kept.ts` (`muster.sessions.folds`) on every fold change: `window.json` through the Rust side, or local storage in mock builds.
- Store: `select("all", id)`, `keysTo`, `showPane`, `showTerminal`, `show`, `setHints` (cleared on deactivate), `tell`, `openFlow(...)`, `openMenuAt(null)`, `composed()`.
- Keys are bound only while `active`. During a drag, the line under the pointer is found with `document.elementFromPoint`, and the composer is focused when compose is requested.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/screens/sessions`
