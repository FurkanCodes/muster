# Muster desktop app Blueprint

## System Overview

Muster's desktop app (`PRODUCT.md` §6a, `ARCHITECTURE.md` §5c: `ARC-24`,
`ARC-25`, `ARC-26`). It is one more client of the daemon, like the terminal
UI. There are two halves:

- **Webview:** `src/`, in React 19 with the React Compiler, Zustand and Base UI. It draws what the daemon's model says and never works state out.
- **Rust:** `src-tauri/`, a Tauri 2 crate in a Cargo workspace of its own. It holds the one daemon connection and the model, and sends whole-entity patches and raw terminal frames to the webview.

The plan, the branches and who owns what are in
[`./PLAN.md`](./PLAN.md). The design system (tokens, components, states) is
[`./design/system/README.md`](./design/system/README.md).

## Local Interface & Invariants

- `package.json` — the scripts:
  - `dev` (Vite on port 1420) and `dev:mock` (the window over the pretend daemon, in a browser);
  - `build`, `build:mock` and `preview:mock`;
  - `typecheck` (three tsconfigs), `lint` and `format` (Biome);
  - `test` (Vitest), `e2e` (Playwright) and `e2e:app` (the real app over WebdriverIO);
  - `bindings`, `replies` and `tauri`.
- `vite.config.ts` — React plus the React Compiler with `panicThreshold: "all_errors"` (a component it cannot compile fails the build: `ARC-25`). It defines `__MOCK__` (mode `mock` or `test`) and `__E2E__` (`MUSTER_E2E_BUILD=1`), builds for `safari16` without source maps, and runs Vitest in jsdom over `src/**/*.test.{ts,tsx}` with `src/test/setup.ts`.
- `playwright.config.ts` — the window end to end in WebKit over a mock build, driven through `window.__muster`. Each worktree gets its own port (from a hash of the folder, or `E2E_PORT`).
- `wdio.conf.ts` — the real app (built `--features e2e`) over a real daemon of its own, through its embedded WebDriver server. Each spec gets its own sandbox, and nothing reaches the person's own Muster.
- `biome.json`, `tsconfig*.json`, `index.html` — lint, type and page setup.
- Every component is built on Base UI. Screens never hand-make a `<button>`, `<input>` or `<dialog>`.
- Types come from Rust (`src/ipc/generated`), never by hand.
- The app never fetches anything from outside (`PRD-10`). Fonts are bundled, and the CSP allows only `self`, IPC, inline styles and `data:` images.

## Subdirectories & System Boundaries

- src: [`./src/README.md`](./src/README.md) — the webview: app shell, flows, IPC bridge and pretend daemon, keys, panels, screens, session pane, spaces, stores, terminals, and the `ui` component library.
- src-tauri: [`./src-tauri/README.md`](./src-tauri/README.md) — the Rust crate `muster-desktop`: the daemon link, the commands, the webview contract, and the outside-world actions.
- scripts: [`./scripts/README.md`](./scripts/README.md) — `bindings.sh` (`pnpm bindings`) and `measure-idle.sh` (CPU and memory at rest, `ARC-25`).
- e2e: the Playwright suite (`*.spec.ts`), run against the mock build in WebKit (`pnpm e2e`).
- e2e-app: [`./e2e-app/README.md`](./e2e-app/README.md) — the real app's end-to-end tests (`pnpm e2e:app`) and their `support/` harness.
- design: [`./design/system/README.md`](./design/system/README.md) — the design system.

## Global Invariants

- `ARC-24`: the app is a client of the daemon only; the webview draws, and Rust works out nothing the model does not.
- `ARC-25`: small, fast and frugal. Hidden means idle (no CPU, no GPU context, nothing streamed). Measure with `scripts/measure-idle.sh`.
- `ARC-26`: screens compose `src/ui`. A component's `mu-*` classes are written only in its own file.
- `PRD-09`: quitting the app leaves the daemon and its sessions running.
- `src-tauri` is its own Cargo workspace: run cargo there for it, and from the root for the core.

## Global Side Effects

- The daemon's Unix socket (started with `muster daemon --detach` when nothing answers).
- `window.json` in the app data folder; `daemon.log` and `settings.toml` opened on request; sounds via `afplay`; URLs via `open`.
- The e2e suites start daemons and apps in sandboxes of their own and remove them afterwards.

## System Verification

- Webview: `pnpm install`, then `pnpm lint`, `pnpm typecheck`, `pnpm test` (or `pnpm exec vitest run <path>`); `pnpm e2e` for a screen's flow.
- Rust (in `src-tauri`): `cargo test`; `pnpm bindings` if a type the webview uses changed.
- Before a release only: `pnpm e2e:app` (the real app, on a Mac).
