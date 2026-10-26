# panels

## Overview

Lays out the panels of a session's pane (`PRODUCT.md` §6/§6a, `ARCHITECTURE.md`
§5a). Panels are described by a small text layout language shared with the TUI
and the daemon, e.g. `list | session 60% | (changes / plugin)`: `|` is side by
side, `/` is stacked, brackets nest, and a size is a share (`60%`) or a count of
cells. Placed sessions appear as `session:ses_<ULID>`.

`layout.ts` parses and edits the tree. `solve.ts` turns it into pixel rectangles,
dividers and drop landings. `Panels.tsx` measures the pane, solves it, and handles
dragging, zooming and dropping. `beside.ts` handles shells and plugin panels, and
saves the layout through the daemon. `carry.ts` carries sidebar lines to drop
targets by pointer.

## Interface

- `Panels` (Panels.tsx) — lays out, drags (4px threshold), zooms and drops the panels of a session's pane. When the panels don't fit, or one is zoomed, it shows only the panel with the keys and counts how many are hidden.
- `TerminalAlone`, `PlacedPanel`, `SessionPanel`, `PluginPanel`, `PanelLine` (PanelBoxes.tsx), `ShellButton` (PaneActions.tsx) — the panel boxes and their head actions.
- `Layout` (layout.ts) — the parsed layout, with `parse`, `default`, `read`, `equals`, `placed`, `placing`, `unplaced`, `moved`, `movedToEdgeOf`, `swapped`, `without`, `sized`, `splitBetween`, `shown` and `toString`.
- `PanelName`, `OwnPanel`, `PlacedPanelName`, `Place`, `LayoutNode`, `LayoutError`, `PANELS`, `panelsIn`, `contains` (layout.ts) — panel ids (`list|session|changes|plugin|session:<id>`), tree types, and a parse error worded for the person, as the daemon words it.
- `DEFAULT_LAYOUT`, `placedPanel`, `placedSession` (layout.ts) — the default line; convert between a `SessionId` and its panel name.
- `solve`, `needs` (solve.ts) — tree to rects plus dividers; the least size a subtree needs.
- `landingAt`, `shareAt` (solve.ts) — where a drop lands (pane edge, beside a panel, or swap on a panel's middle quarter); a divider's share at a pointer, clamped to 10–90%.
- `GAP`, `CELL`, `PANEL_LEAST`, `EDGE_ZONE` (solve.ts) — 8px gap, 8×20 cell, 320×140 least panel, 24px edge zone.
- `panelBeside`, `placedOrTaken`, `openShell`, `startShell`, `togglePanel`, `putPanelAway`, `running`, `giveKeys` (beside.ts) — plugin panel beside a session, place/take a session, the `T` workspace shell, plugin panel toggles.
- `keptLayout`, `keepLayout` (beside.ts) — read the layout from preferences; save it.
- `pressToCarry`, `useCarrying`, `onCarrying`, `useTakesCarried`, `Carried`, `Carrying` (carry.ts) — carry a sidebar line by pointer and let drop targets take it.
- Hooks: `useLayout`, `useArranging` (with `neighbour`), `usePanelKeys` (with `usePanelHandlers` and `toggleFullScreen`), `usePrefixKeys` — the layout state, arranging mode and panel/prefix keys.

## Invariants

- Every layout holds each own panel (`list`, `session`, `changes`, `plugin`) exactly once. A missing one is put where the default layout puts it, and the retired `shell` panel is dropped.
- Mixing `|` and `/` in one run without brackets is a `LayoutError`. Shares are 1–99 and cells 1–65535. A placed session's id must match `ses_` plus a 26-character Crockford ULID.
- The desktop app and the TUI write the same layout line, which the daemon checks the same way.
- Moving a panel never restarts its terminal: each panel is positioned by its own rectangle inside one box.
- Every terminal on the pane is drawn once and none twice: the preview keeps the last shown session that is not placed.
- A shell that has ended leaves its panel (the daemon removes it from the layout).
- Carrying uses pointer events, never HTML5 drag and drop, which the window takes for OS drags.

## Side Effects

- IPC: `ChangePreference { Panels }` (keepLayout); `StartSession` (harness `shell`, 80×24), `StopSession`, `RemoveSession` once the shell has ended; `Plugin.OpenPanel`.
- Store: reads `daemon.store` sessions, preferences and plugins; UI actions `showTerminal`, `keysTo`, `tell`.
- DOM: a `ResizeObserver` on the pane, pointer capture, a capture-phase Esc handler that cancels a drag. `carry.ts` adds window pointer and key listeners, sets the `is-carried`/`is-carrying` classes, swallows the click that ends a carry, and keeps module-level state.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/panels`
