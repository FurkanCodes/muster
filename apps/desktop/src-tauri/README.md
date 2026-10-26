# src-tauri (muster-desktop)

## Overview

The Rust half of Muster's desktop app: the crate `muster-desktop`, a Tauri 2
shell around the system webview. It is a Cargo workspace of its own (`ARC-24`).
The webview's crates never reach the core's builds, checks or supply-chain
rules, and the core crates (`muster-client`, `muster-core` with feature `ts`,
`muster-model`) are reached by path. Run cargo here for it, and from the
repository root for the core.

## Interface

- `src/` — the library `muster_desktop` and the `muster-desktop` binary: [`./src/README.md`](./src/README.md).
- `build.rs` — `tauri_build::build()`.
- `Cargo.toml` — the crate and its own `[workspace]`. The feature `e2e` adds the embedded WebDriver server (`tauri-plugin-wdio-webdriver`) and the AppKit bindings for ⌘Q; it is only in builds for `pnpm e2e:app` and is never shipped.
- `tauri.conf.json` — product `Muster`, identifier `dev.muster.desktop`; the `main` window (1440×900, at least 960×600, overlay title bar); a strict CSP (`self`, IPC connects, inline styles, `data:` images); bundles `app` and `dmg` for macOS 13+.
- `capabilities/default.json`, `permissions/{folders,muster,settings}.toml` — which commands the webview may invoke.
- `icons/` — the app's icons.
- `tests/` — integration tests (`link.rs`, `permissions.rs`, `replies.rs`, `window.rs`, `support/`).

## Invariants

- `ARC-24`: a workspace of its own, and a client of the daemon only.
- `ARC-25`: small, fast and frugal. Release builds use one codegen unit, LTO, `panic = "abort"` and stripped symbols.
- `unsafe_code = "forbid"`; clippy pedantic, `unwrap_used` and `expect_used` warn.
- The CSP lets the page load nothing from outside (`PRD-10`).
- Version numbers match `apps/desktop/package.json` and `tauri.conf.json`.

## Side Effects

- Building runs `tauri-build`. `tauri.conf.json` runs `pnpm dev` before dev and `pnpm build` before a build.
- `pnpm replies` (`MUSTER_WRITE_REPLIES=1 cargo test --test replies`) rewrites the mock's `src/ipc/mock/replies.json`.

## Verification

- Run (here): `cargo test`; `cargo test --features e2e` for e2e-only code.
- After changing a type the webview uses: `pnpm bindings` in `apps/desktop`.
