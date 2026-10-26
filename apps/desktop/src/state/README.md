# state

## Overview

The window's two vanilla Zustand stores and their helpers. `createDaemon()`
holds the daemon's state as the webview sees it: `link`, `loaded`, projects,
workspaces and sessions by id, preferences, spaces, plugins, resources,
notifier, and the Rust-side `Derived`. `createUi()` holds the window's own
state: screen, list, selection, `keysIn`, terminal, pane, full screen, sidebar
fold, theme, palette, flow, hints, note, visibility, menu, space, and
`changesTurn`, `compose`, `drawnIn`, `lastAgent`, `spacesOpen`. `kept.ts` is
the window's small persistence layer.

## Interface

- `createDaemon`, `DaemonState`, `DaemonStore`, `Daemon`, `emptyDerived` (daemon.ts) — the daemon store. `apply(message)` handles `link`, `patch` and `reveal` (which notifies the `onReveal` listeners).
- `createUi`, `UiState`, `UiActions`, `UiStore` (ui.ts) — the UI store; its actions are plain setters, plus `tell` and `cycle`.
- `ScreenId`, `ListId`, `TABS`, `isList`, `KeysIn`, `Theme`, `PaletteMode`, `PaneMode`, `Hint`, `Note` (ui.ts) — UI value types and the Tab cycle's order.
- `kept`, `keep` (kept.ts) — read and write kept window state (`null` deletes).
- `keepIn`, `Keeper`, `KeptInApp` (kept.ts) — switch keeping from local storage to the app's Rust side (`kept.rs`) after loading its values.
- `shownHarness`, `standsBeside`, `chats` (selectors.ts) — the agent shown in front; a shell or plugin panel against a chat; chats only.
- `projectsInOrder`, `projectColour`, `workspaceOf`, `projectOf`, `branchOf` (selectors.ts) — ordering, colour 1–6, parent lookups, branch name.
- `whenKnown` (whenKnown.ts) — run a callback once an id exists in projects, workspaces or sessions; returns a stop function.

## Invariants

- `ARC-25`: a patch replaces only the entities it names and keeps every other object identical, so a selector re-renders only when its own entity changes. A `reset` patch replaces every entity map.
- `ARC-28`: the harness shown is `session.agent?.harness` (an agent running in front in a terminal), else `session.harness`.
- `ARC-33`: a terminal in no project has `workspace === null`. `ui.space` is the window's own, and every window opens on everything (`null`).
- UI state is never the daemon's. There is one UI store, so every screen reads the same answers.
- Showing a screen resets `keysIn` to `list` and clears full screen and the menu. `openPalette(true)` closes any flow, and `openFlow` closes the palette.
- A note that is not an error clears after 5 s. An error note stays until the next key.
- The theme is dark unless `light` or `system` is kept.
- `whenKnown` uses no timers; it gives up after 200 store changes.

## Side Effects

- Store `setState` for every action and message; the reveal listeners are called on `reveal`.
- Local storage (inside try/catch) by default and in mock builds; after `keepIn`, `KeptInApp.keep` over IPC (errors swallowed). Kept keys include `muster.theme`.
- `setTimeout(5000)` for a note's fade.
- `whenKnown` subscribes to the daemon store until it finds the id or gives up.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/state`
