# apps

## Overview

Applications built on top of the Rust core in [`crates/`](../crates/README.md). Today there is one: the desktop app, a window over the same daemon the terminal UI uses (`PRODUCT.md` §6a, `ARCHITECTURE.md` §5c). It is a daemon client only and owns no state of its own (`ARC-24`).

## Subdirectories & Boundaries

- desktop: [./desktop/README.md](./desktop/README.md) - Muster's desktop app: a React 19 webview (`src/`, Zustand stores, the Base UI component library in `src/ui`, `ARC-26`) and a Tauri 2 Rust crate (`src-tauri/`, a Cargo workspace of its own) that holds the one daemon connection, sends whole-entity patches at most every 16 ms and never sends terminal output through JSON (`ARC-25`). Quitting leaves the daemon running (`PRD-09`); nothing is fetched from outside (`PRD-10`).
