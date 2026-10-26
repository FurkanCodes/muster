# src (desktop webview)

## Overview

The webview half of Muster's desktop app: React 19 (with the React Compiler),
Zustand and Base UI, drawn in the Mac's WebKit webview. It draws and never works
state out (`ARC-24`). The daemon's state and its derived orderings arrive as
patches from the Rust side, through the one `Bridge` in `ipc`. Screens compose
the components in `ui` and the `Page` and `Sidebar` in `app` (`ARC-26`).

## Local Interface & Invariants

- `main.tsx` — the entry. It imports the bundled fonts (Instrument Sans, JetBrains Mono; Latin only) and the styles (`styles/tokens.css`, `components.css`, `app.css`), awaits `createBridge()`, and renders `<App bridge={bridge} />` under `StrictMode`.
- `env.d.ts` — the build flags: `__MOCK__` (mock builds and tests use the pretend daemon) and `__E2E__` (the app built for `pnpm e2e:app`, never shipped).
- `PRD-10`: fonts ship with the app, which never fetches anything.
- Types of the protocol come from `ipc/generated` (written by `pnpm bindings`), never by hand.

## Subdirectories & System Boundaries

- app: [`./app/README.md`](./app/README.md) — the window shell: stores made once, wiring, the window key listener, sidebar, status bar, page parts, screen registry, visibility and clock.
- flows: [`./flows/README.md`](./flows/README.md) — the ⌘K palette and the dialogs, and the TUI's verbs on the selection (add or clone a project, worktrees, close, stop, restart, tidy, move).
- ipc: [`./ipc/README.md`](./ipc/README.md) — the `Bridge` (Tauri invoke and channels), binary terminal frames, `ask`/`useAsked`, and the pretend daemon in `ipc/mock`.
- keys: [`./keys/README.md`](./keys/README.md) — the port of `muster_core::keys`: chords, the person's keymap over the defaults, plugin panel keys, the panel-key prefix mode.
- lib: [`./lib/README.md`](./lib/README.md) — pure helpers: agent names and logos, status words, formats, text.
- panels: [`./panels/README.md`](./panels/README.md) — the layout language shared with the TUI and daemon, solving it into rectangles, dragging and dropping panels, shells and plugin panels.
- screens: [`./screens/README.md`](./screens/README.md) — the *Needs you*, All sessions/project page, Settings and Tickets screens.
- session: [`./session/README.md`](./session/README.md) — the session pane, shared session actions and menu, by-hand stage tools, and the Changes board.
- spaces: [`./spaces/README.md`](./spaces/README.md) — spaces (`ARC-33`): where new things go, space-first deferral, terminals in no project, move menus.
- state: [`./state/README.md`](./state/README.md) — the daemon and UI Zustand stores, kept window state, selectors, `whenKnown`.
- terminal: [`./terminal/README.md`](./terminal/README.md) — kept xterm.js terminals per session, leases, WebGL on demand, wheel reports, theme colours.
- ui: [`./ui/README.md`](./ui/README.md) — the Base UI component library (`ARC-26`).
- Not documented here: `assets/` (agent logos), `styles/` (tokens and global CSS), `test/` (vitest setup), `ipc/generated/` (generated types).

## Global Invariants

- `ARC-24`: the webview never sorts or derives what the model already gives.
- `ARC-25`: while the window is hidden nothing animates, ticks, streams or asks. Patches keep unchanged objects identical, so components re-render only for their own part. Heavy screens and dialogs load lazily.
- `ARC-26`: a component's `mu-*` classes are written only in the file that defines it. A list the keyboard moves through is a `List`.
- Only `ipc` calls Tauri.

## Global Side Effects

- The Tauri bridge to the Rust side (`src-tauri`), and through it the daemon's socket.
- Window state kept in `window.json` through the Rust side, or local storage in mock builds.

## System Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm test` (or `pnpm exec vitest run <path>` for what changed).
