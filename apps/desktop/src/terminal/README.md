# terminal

## Overview

The session terminals, drawn with xterm.js. Each session has one long-lived
xterm (`KeptTerminal`), held in a per-app `Keeper`, and each showing borrows it
through a lease. When a terminal goes off screen it is kept idle. The least
recently shown ones beyond `IDLE_TERMINALS` are disposed. Moving a terminal
between panes moves its DOM element, so it is never rebuilt or replayed.
`Gpu` holds a WebGL context only while the terminal is shown and the window is
seen.

## Interface

- `Terminal` (Terminal.tsx) — shows a session's kept xterm. It fits the xterm, tells the daemon real size changes, attaches from `nextSeq` while fitted and visible (detaches otherwise), and handles focus, keys and the theme from CSS tokens.
- `KeptTerminal` (kept.ts) — xterm + FitAddon + `Gpu` for one session: `showIn`, `putAway`, `setCompact`, `startFeed`, `stopFeed`, `dispose`; `nextSeq`, `modes`, `told`.
- `keptTerminals` (kept.ts) — the per-App Keeper (a WeakMap). It prunes removed sessions and clears on `pagehide`.
- `IDLE_TERMINALS` (kept.ts) — at most 4 off-screen terminals kept.
- `loadWebgl`, `Feed` (kept.ts) — loads the WebGL addon (disposing it if loading throws); the frame sink whose stale frames are dropped.
- `Keeper<T>`, `Lease<T>`, `Keepable` (keeper.ts) — lease-based cache with least-recently-shown idle eviction: `take`, `give`, `keepOnly`, `clear`, `state`.
- `Gpu`, `MOST_RETRIES`, `GpuAddon`, `HeldContext`, `Frames` (gpu.ts) — WebGL lifecycle (`want`, `see`, `drawing`, `retrying`, `dispose`); up to 3 retries per showing, then the DOM renderer; injectable seams.
- `WheelLines`, `wheelReport` (wheel.ts) — wheel deltas summed into whole lines (±100 at most, restarting on a change of direction); SGR, UTF-8 and X10 wheel reports for programs that take the mouse.
- `terminalColours`, `rgbOf` (colours.ts) — the theme's foreground, background and 16-colour palette as X11 16-bit RGB, for the daemon's colours frame (OSC 10/11/4 replies).
- `testing.ts` — test hooks (`exposeForTests`).

## Invariants

- `ARC-25`: bytes stream as they come. An off-screen or 0×0 terminal streams nothing and holds no GPU context. Work is driven by frames, not timers, and nothing polls.
- A held lease is never evicted except by `clear`. A taken-over lease's `give` does nothing. A key removed while held is let go once given back.
- An xterm is fed only after it has been fitted to its pane at least once; otherwise the program's screen is scrambled.
- The program is told only real, changed sizes, never xterm's default 80×24 or a 0×0 pane.
- Frames from a stale feed, or after dispose, are dropped. A replay with `reset` (or the first replay) resets xterm.
- ⌘ shortcuts, ⇧Q and window/panel-key chords never reach the program.
- A lost or failed GPU context is retried only while wanted and seen, on animation frames. When the GPU is let go, the context is lost on purpose, because WebKit limits contexts.

## Side Effects

- IPC: `bridge.resize`, `bridge.attach(session, nextSeq, true, onFrame)`, `bridge.detach`, `bridge.input` (typed keys and wheel reports).
- Store: `showTerminal`, `keysTo("terminal")`; a daemon store subscription that prunes kept terminals of removed sessions.
- DOM: xterm element moves, `ResizeObserver`, `requestAnimationFrame`, `document.fonts` `loadingdone`, window `pagehide`, `getComputedStyle` of `--surface-term`, `--text` and `--text-selection`.
- WebGL: addon load and dispose, `WEBGL_lose_context.loseContext()`, one `console.warn` on falling back to the DOM renderer.
- Writes DECSET mode sequences (`?1`, `?2004`) into xterm when the daemon reports modes.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/terminal`
