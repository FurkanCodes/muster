# spaces

## Overview

Spaces group projects and terminals in no project (`ARC-33`, `PRODUCT.md`
§6a). A `Member` is `{Project: id}` or `{Session: id}`. `spaces.ts` does several
things:

- finds the space shown and which space a line belongs to;
- picks where something new goes;
- with no space yet, asks for a name first and then runs the deferred work in the new space;
- wraps `bridge.command` with done or failed notes;
- starts shells in no project.

Nothing here filters a list: what a space lists comes from the model's
`Derived.space_rows`.

## Interface

- `spaceShown` (spaces.ts) — the `Space` for `ui.space`, or null for everything.
- `spaceOfLine` (spaces.ts) — the space holding a line (a space, project, workspace's project, or a terminal in no project).
- `spaceToJoin` (spaces.ts) — where new things go: the selected line's space, else the one shown, else the first.
- `spaceFirst`, `doLater` (spaces.ts) — with no spaces, open the space-name flow carrying `Later`; then run it (terminal, add-project, clone-project, move) in the new space.
- `spacesOf` (spaces.ts) — the names of the spaces a member is in.
- `command`, `moveTo` (spaces.ts) — `bridge.command` with a note; `MoveToSpace` with "Moved to X."
- `newTerminal`, `terminalIn` (spaces.ts) — start a shell in no project (100×30), then show and select it.
- `openSpaces`, `fileTerminal` (spaces.ts) — `S` opens the space-name flow; `F` files a terminal in no project under a project.
- `SpaceLineMenu` (SpaceMenu.tsx) — the right-click menu on a space heading: open a folder, clone, terminal, rename, remove.
- `moveItems` (moveItems.ts), `useProjectMenu` (useProjectMenu.ts) — "Move to <space>" and "Move to a new space…" items.
- `SpaceNote` (SpaceNote.tsx) — the model's `space_notes` for a line; with `always=false`, only while everything is shown.

## Invariants

- `ARC-33`: a project is always in exactly one space, and a terminal in no project in one or none (`terminalIn(app, null)` starts one in none, and `spaceToJoin` can return null). A space only groups. `ui.space` belongs to the window and is never the daemon's or kept.
- `moveItems` leaves out the spaces the member is already in. Members are compared as JSON strings.
- `doLater` exists because the daemon's reply can arrive before the patch that adds the new space or session. `terminalIn` waits with `whenKnown` before selecting.
- `fileTerminal` acts only on a selected session with `workspace === null`; otherwise it shows an error note.

## Side Effects

- IPC: `MoveToSpace`; `StartSession` (`harness: "shell"`, `workspace: null`, 100×30, `space`); `addProjectIn` via flows.
- Store: `tell`, `show("all")`, `select("all", id)`, `openFlow(...)`, `openMenuAt(null)`.
- `whenKnown` subscribes to the daemon store until the new session appears. No local storage, no timers.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/spaces`
