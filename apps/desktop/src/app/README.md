# app

## Overview

The window's shell. `App({bridge})` makes the two stores once (`createDaemon()`,
`createUi()`) and provides them, the bridge and the UI actions through
`AppContext`. One effect connects the window:

- keys, visibility and the queue's history;
- the daemon's reveals (show the session's terminal);
- the clock, which runs only while the window is visible;
- `bridge.open(msg => daemon.apply(msg))`.

The window applies the theme and pushes the terminal colours to the daemon. It
blocks the browser's right-click menu except over text, and tracks Mac full
screen. `Body` fills the main area with one of: `Away` (daemon down, nothing
loaded), `Welcome` (no projects and no sessions: a terminal in a space is shown, not the welcome), a page from `PAGES`, or `Detail`. The sidebar
list is drawn inside React `<Activity>`, so it keeps its state while hidden.

## Interface

- `App` (App.tsx) — the root component.
- `App` (the interface), `AppContext`, `useApp`, `useDaemon`, `useUi`, `useActions`, `useBridge` (context.ts) — `{bridge, daemon, ui, actions}` and narrow selector hooks over the stores.
- `installKeys` (keys.ts) — the one window `keydown` listener: global ⌘ shortcuts (⌘K, ⌘N, ⌘1–4, ⌘, and ⌘\), routing to terminal, dialog or palette.
- `useKeys`, `KeyHandlers`, `KeysHeld` (keys.ts) — a stack of handler refs: the newest handlers win; `beneath` puts handlers at the bottom; `KeysHeld` scopes keys to one part of the window.
- `pressKey`, `chordOf` (keys.ts) — run a chord's handler as if pressed (the palette uses it); name a `KeyboardEvent` as a chord.
- `Sidebar`, `SidebarToggle`, `SidebarAway`, `SidebarAwayRow`, `SIDEBAR_WIDTH` (Sidebar.tsx) — Kep's face, New session, Search and Tickets, the Needs you/All switch, the Spaces heading, a drag-resizable width kept between launches, and the right-click line menu.
- `StatusBar` (StatusBar.tsx) — the note (a live region), typing hint, hints, the selected session's facts, counts, a memory meter and the version.
- `Page`, `PageHeader`, `PageTitle`, `PageMeta`, `PageActions`, `NothingSelected` (Page.tsx) — a pane's layout parts; the header is a window drag region.
- `Detail` (Detail.tsx) — the pane for the selected id: `SessionPane`, `ProjectPage` or `RowDetail`.
- `LISTS`, `PAGES`, `ListScreen` (screens.tsx) — the screen registry.
- `installVisibility` (visibility.ts) — tracks visibility and focus, and tells the daemon who is looking.
- `runClock`, `useNow` (clock.ts) — one 15 s wall clock for the whole window.
- `Connecting` (Connecting.tsx) — loading, or "daemon out of reach".

## Invariants

- `ARC-25`: while the window is not visible, nothing animates, the clock stops, terminals detach, the daemon is told nobody is looking, and Rust sends nothing. There is one clock, ticking every 15 s only while visible. The list and *Needs you* are bundled eagerly; Tickets and Settings load on first show.
- The stores are made once per window life.
- While the terminal has the keys, every key goes to the program except Q/⇧Q (leave), the ⌘ shortcuts, and the panel (prefix) key with the key after it.
- The open palette keeps the keys; only ⌘K closes it. ⌘K and ⌘N do nothing unless the link is up.
- Components read state through narrow selectors, so each re-renders only for its own part.

## Side Effects

- IPC: `bridge.open`, `bridge.colours` on each theme change, `bridge.looking` (deduplicated), and `StartSession` via the sidebar's Terminal button (`spaces`).
- Store: show, select, `showTerminal` and `keysTo` on a reveal; `setVisible`, `foldSidebar`, `cycle`, `openPalette`, `openFlow`, `openMenuAt`, `tell`/`clearNote`.
- Persistence: `muster.sidebar-width` through `state/kept`.
- Timers: the clock's 15 s interval, cleared when hidden.
- DOM: `data-theme`, `data-window-full`, `data-visible` and `data-resizing` on `<html>`; listeners for `keydown`, `contextmenu`, `resize`, `visibilitychange`, `focus`, `blur` and `prefers-color-scheme`.
- Module-level state: the key handler stack and holds; the clock's listeners and timer.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/app`
