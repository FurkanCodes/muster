# apps

## Overview

Applications built on top of the Rust core in [`crates/`](../crates/README.md). The desktop app is a window over the same daemon the terminal UI uses (`PRODUCT.md` §6a, `ARCHITECTURE.md` §5c). It is a daemon client only and owns no state of its own (`ARC-24`). Beside it are the README's videos, made from the desktop app and the terminal UI.

## Subdirectories & Boundaries

- desktop: [./desktop/README.md](./desktop/README.md) - Muster's desktop app: a React 19 webview (`src/`, Zustand stores, the Base UI component library in `src/ui`, `ARC-26`) and a Tauri 2 Rust crate (`src-tauri/`, a Cargo workspace of its own) that holds the one daemon connection, sends whole-entity patches at most every 16 ms and never sends terminal output through JSON (`ARC-25`). Quitting leaves the daemon running (`PRD-09`); nothing is fetched from outside (`PRD-10`).
- video: [./video/README.md](./video/README.md) - the README's short videos: the desktop app's real window, recorded over its pretend daemon (`kit/`), and the terminal UI's real screen, the `muster` binary in a real terminal over a throwaway daemon (`kit/tui/`), each cut on a beat grid by a page whose every frame is a function of time (`launch/`). `pnpm media` writes them to `docs/media`. It is not part of Muster and ships in no release.
