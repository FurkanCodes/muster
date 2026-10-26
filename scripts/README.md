# scripts

## Overview

Helper scripts for the repository: CI rule checks, agent-session
measurement, Linux builds, logo rendering and a daemon soak test. Run them
from the repository root. The `check-*` scripts are CI steps
(`.github/workflows/ci.yml`) and exit non-zero with a message saying what
failed; most name the rule they enforce (`check-no-secrets.sh` names it only
in a comment).

## Interface

- `check-file-size.sh [limit=1000]` — fails on a source file over the line limit: core Rust, desktop Rust and TypeScript, generated bindings excluded (`ARC-08`).
- `check-type-size.sh [limit=40]` — fails on a Rust struct (core and desktop) with more fields than the limit (`ARC-08`).
- `check-no-secrets.sh` — fails if any tracked file matches a real provider token format (GitHub, GitLab, Anthropic, OpenAI, Slack, AWS, private keys, Atlassian, Linear, Bitbucket); only `crates/muster-core/src/credentials.rs` is allowed (`ARC-20`, `PRD-06`).
- `check-test-sleeps.sh` — fails where an end-to-end test (`crates/muster/tests`, `apps/desktop/src-tauri/tests`) sleeps instead of waiting through `support/wait.rs` (`ARC-30`).
- `check-tui-colours.sh` — fails where `muster-tui` names a `Color::` outside `view/theme.rs`, `view/kep.rs`, `view/logos.rs` and tests (`ARC-29`).
- `check-duplication.py [window=12]` — fails where the same run of non-trivial lines appears twice in core Rust, desktop Rust or TypeScript; tests, generated and mock code are skipped (`ARC-31`).
- `agent-sessions.py [out.json] [session id to skip]` — measures this checkout's Claude Code transcripts (time by tool and command, whole-suite runs, reruns, failures, polling, subagents, tokens) into JSON (default `agent-sessions.json`); run as `python3 -I scripts/agent-sessions.py`.
- `linux-build.sh [x86_64|aarch64]` — builds `muster` for Linux in an Ubuntu 22.04 Docker container into `target/linux/muster-<arch>`, for `MUSTER_REMOTE_BINARY=... muster ssh <machine> --install` (`ARC-27`).
- `render-logos.sh` — renders `crates/muster-tui/assets/logos/*.svg` to 64-px PNGs and `.rgba` pixel files (needs `rsvg-convert` and python3).
- `soak.sh [minutes=10] [muster binary]` — runs a sandboxed daemon under client/session churn and samples its memory and `muster status` counts, to spot leaks (`ARC-13`, `ARC-14`).
- [hooks](./hooks/README.md) — Claude Code hooks (`whole-suite.py`).

## Invariants

- The `check-*` scripts only read the tree; they never change files.
- `agent-sessions.py` output quotes commands and the person's words, so it stays local and is never committed.
- `soak.sh` never touches the person's own daemon: runtime and data dirs live under a temporary folder.
- `linux-build.sh` mounts the source read-only; toolchain and build cache live in Docker volumes.

## Side Effects

- `agent-sessions.py` reads `~/.claude/projects/<this checkout>` and writes its JSON file.
- `linux-build.sh` runs Docker (downloads Ubuntu, rustup) and writes `target/linux/`.
- `render-logos.sh` overwrites the PNG and `.rgba` files beside the logo SVGs.
- `soak.sh` starts and stops a daemon and clients in a temporary folder.

## Verification

- No test runner. CI runs each `check-*` script from the root (e.g. `scripts/check-file-size.sh`, `python3 scripts/check-duplication.py`); the others are run by hand as above.
