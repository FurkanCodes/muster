# link

## Overview

The submodules of `../link.rs`: the desktop app's one connection to the
daemon (`ARCHITECTURE.md` §5c). `Link::new` (in `link.rs`) returns a cheap,
cloneable handle and a future for the caller to spawn and own. That future is
`actor::run`, a single-owner loop. It takes the webview's requests from a
channel and daemon frames from a bounded queue, and runs two frame-rate
deadlines, one for state patches and one for held terminal output.

The daemon's state lives in a `muster_model::Model`, the same model the terminal
UI keeps. `patches` diffs it against what the webview was last sent. `streams`
shares each session's terminal stream among its readers. `questions` reads the
vt100 screen behind a session's *Needs you* question. `daemon` finds and starts
the `muster` binary.

## Interface

- `actor::run` (actor.rs, `pub(super)`) — the link's select loop over requests, link events, the patch deadline and the output deadline. A spawned `keep_linked` task connects through `muster_client::Connection`, retries every `RETRY_EVERY` (500 ms) or at once on `try_now`, and starts the daemon when nothing answers.
- `find_muster` (daemon.rs) — the `muster` binary: `MUSTER_BIN`, then `~/.local/bin`, `~/.cargo/bin`, then absolute `PATH` folders.
- `installed_muster` (daemon.rs) — `find_muster` over the process's environment.
- `starter` (daemon.rs) — a `Start` that runs `muster daemon --detach`, with stderr appended to `daemon.log`. The `PATH` it gets joins the app's own, the login shell's (`$SHELL -i -l`, 3 s timeout) and muster's own folder.
- `Patcher`, `derive` (patches.rs, `pub(super)`) — whole-entity patches plus derived data (needs-you order, rows, counts, questions) from the model.
- `Streams` (streams.rs, `pub(super)`) — per-session terminal streams with a cursor per reader. A stream is asked for from the earliest place any reader holds. Output for the webview's terminal is batched per frame.
- `LINES`, `last_lines` (questions.rs, `pub(crate)`) — the last 4 non-blank lines of a vt100 screen, used as a session's question; `Screen` (`pub(super)`) is the reader `streams.rs` feeds.
- Re-exported by `link.rs`: `find_muster`, `installed_muster`, `starter`.

## Invariants

- `ARC-13`: every spawned task is owned and aborted on drop; the caller owns the link future, which ends when every handle is dropped.
- `ARC-14`: unbounded channels carry only the webview's requests. Daemon frames are read into a bounded queue (64), so a slow link pushes back. Pending commands and stream state are dropped when the connection goes or their sessions disappear.
- `ARC-24`: patches carry whole entities, removals and derived data; the webview never works state out.
- `ARC-25`: at most one patch, and one batch of terminal output, per frame (16 ms). Nothing is sent while the window is not seen.
- `ARC-03`: `link::now` and `link::monotonic` are the link's clocks, but `actor.rs` also reads `tokio::time::Instant::now()` (and `elapsed()`) directly for its deadlines.
- `ARC-30`: sleeps poll only where nothing else signals: the retry interval, the 50 ms socket poll for up to `START_WAIT` (5 s) after a start, and the login shell poll.
- A daemon is not started again within `START_AGAIN_AFTER` (10 s) of the last start.
- `PRD-09`: the daemon is started detached, so closing the app leaves it and its sessions running.

## Side Effects

- A Unix socket connection to the daemon (`muster-desktop <version>`), with a spawned writer task for outgoing frames: commands, input, resize, looking, keys, colours and stream requests.
- Tokio tasks (the actor, `keep_linked`, the per-connection writer) and `spawn_blocking` for the daemon start.
- Subprocesses: `muster daemon --detach` (stdin and stdout null, stderr to a file, `PATH` overridden) and `$SHELL -i -l` for the login `PATH`, killed after 3 s. An OS thread, `muster-reap-daemon`, waits on the detached child so it does not linger as a zombie.
- Files: creates `runtime_dir` (mode 0700) and appends to `<runtime_dir>/daemon.log`.
- To the webview through `StateSink`: `StateMessage::Link` (Connecting, Up, Down), `Patch` (with `reset` on open and reconnect), and `Reveal`. Through `FrameSink`: raw terminal frames.
- Reads `MUSTER_BIN`, `HOME`, `PATH` and `SHELL`.

## Verification

- Run (in `apps/desktop/src-tauri`): `cargo test link` (unit tests in `actor_tests.rs`, `patches_tests.rs`, `streams_tests.rs`; end-to-end `tests/link.rs`)
