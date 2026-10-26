# muster-daemon/src

## Overview

The Muster daemon: it owns every session and run, and is the only process
that talks to agents, Docker, GitHub and ticket sources (`ARCHITECTURE.md`
§4). `lib.rs` holds the crate's public API: `run` starts the daemon (its two
Unix sockets, its lock, the SQLite store, the Supervisor, the watchers and the
hook receiver) and runs it until told to stop or to restart in place. The
`muster` CLI crate is the program that calls it; there is no `main.rs` here.

The Supervisor (`supervisor.rs`) owns all state (`ARC-02`); the other modules
are the systems it reaches through traits (`ARC-03`) and the tasks that feed
it. A Rust `foo.rs` plus `foo/` pair means `foo/` holds `foo.rs`'s
submodules; each such directory has its own README.

## Interface

Public API (`lib.rs`):

- `run(config, stops, relaunch, adopt)` (lib.rs) — runs the daemon until a
  `Stop`; on `Stop::Reload` it restarts in place (`handoff`), with `adopt` it
  takes over from the previous version.
- `DaemonConfig`, `DaemonError`, `Stop`, `HANDOFF_FORMAT` (lib.rs) — the
  daemon's settings, failures, stop reasons and handoff format.
- `sign_in`, `SignIn`, `keep_secret`, `keep_jira_token`, `jira_site`,
  `bitbucket_account`, `plugin_account`, `LINEAR_KEY_ACCOUNT` (lib.rs) — what
  `muster auth` uses to keep tokens and sign in to MCP servers.
- `plugin_run`, `PluginRunError` (plugin_run.rs) — `muster plugin-run <id>`:
  Muster's own ticket plugins served over stdin and stdout.
- `ServiceError` (service.rs), `LogLimits` (log_files.rs),
  `KEYCHAIN_SERVICE` / `KEYCHAIN_ACCOUNT` (runs/credentials.rs) — re-exported.
- `pub mod clock` — `Clock`, `SystemClock`: the clock behind a trait
  (`ARC-03`).
- `pub mod config` — `load`, `load_with_sources`, `change`, `preferences`,
  `own_settings`, `change_preference`, `plugin_entries`, `change_plugin`,
  `FILE_NAME` (`muster.toml`), and the structs they return, `Loaded`,
  `Sources` and `OwnSettings`: finding and writing settings files
  (`ARCHITECTURE.md` §12); parsing is `muster_core::config`'s.
- `pub mod store` — the SQLite store: [store/README.md](./store/README.md).

Modules with subdirectories:

- [supervisor/README.md](./supervisor/README.md) — the Supervisor actor and its
  responsibilities (`ARC-02`, `ARC-10`).
- [server/README.md](./server/README.md) — the client socket server and
  per-connection outbox.
- [pty/README.md](./pty/README.md) — sessions' terminals: actors, ring, screen,
  terminal queries.
- [harness/README.md](./harness/README.md) — what to run for each harness and
  how each agent is heard (`PRD-13`).
- [runs/README.md](./runs/README.md) — runs' work: containers, jobs, mirror,
  prompts (`ARC-04`, `ARC-06`).
- [trackers/README.md](./trackers/README.md) — ticket sources and ticket
  plugins (`PRD-07`).
- [githost/README.md](./githost/README.md) — pull requests on GitHub, GitLab
  and Bitbucket.
- [plugins/README.md](./plugins/README.md) — the plugin registry, changes and
  hooks (`PRD-15`).
- [timeline/README.md](./timeline/README.md) — workspaces' turn timelines,
  undo and branch moves (`PRD-14`).
- [spend/README.md](./spend/README.md) — agents' token use read from their own
  records (`PRD-12`).
- [handoff/README.md](./handoff/README.md) — restart in place; the only unsafe
  code (`ARC-12`).

Other top-level modules (no subdirectory README; `containers/`, `mcp/`,
`oauth/`, `plugin_run/` and `terminal/` hold only tests):

| File | Role |
|---|---|
| `hooks.rs` | Hook receiver on the hook socket: `/hooks/<harness>[/<event>]` |
| `terminal.rs` | Streaming one session's terminal to one client |
| `git.rs`, `process.rs` | Running git and other programs: no shell, time limit, own process group, killed whole |
| `containers.rs` | Containers through the `docker` command (`ARCHITECTURE.md` §8) |
| `github.rs`, `glab.rs`, `repositories.rs` | The person's `gh` and `glab`; repositories they can clone |
| `mcp.rs`, `oauth.rs`, `secrets.rs` | MCP client over Streamable HTTP; OAuth 2.1 browser sign-in; tokens in the macOS Keychain (`PRD-06`) |
| `plugin_run.rs`, `service.rs` | Built-in ticket plugins served; futures and errors trackers and hosts share |
| `branches.rs`, `changes.rs`, `clashes.rs`, `tidy.rs`, `worktree_sync.rs`, `shared_builds.rs` | Git reads per workspace: branch, changes, clashes, tidy, worktrees made outside Muster, shared build folders |
| `fronts.rs`, `resources.rs`, `agents_found.rs`, `ask.rs` | Watchers: foreground program (`ARC-28`), resource use (`PRD-05`); installed agents; asking an agent one question |
| `tasks.rs`, `log_files.rs`, `clock.rs`, `config.rs` | Task ownership (`ARC-13`); bounded log (`ARC-15`); clock; settings files |
| `waiting.rs` | Test helper: wait for what must happen, never a guessed sleep (`ARC-30`) |

## Invariants

- The daemon is the only process that talks to agents, Docker, git hosts and
  ticket sources; the Supervisor alone owns its state (`ARC-02`).
- Docker, git hosts, ticket sources, harnesses, PTYs and the clock are reached
  only through their traits (`ARC-03`), with fakes in tests.
- Every task has an owner and work off the loop has a deadline (`ARC-13`);
  everything held is bounded (`ARC-14`); the log is bounded (`ARC-15`).
- External programs run without a shell, in their own process group, under a
  time limit, without the git variables of whatever started the daemon.
- No real credential appears in code, fixtures, logs or test output; tokens
  live in the macOS Keychain or the environment (`PRD-06`).
- No source file over 1,000 lines, no struct over 40 fields (`ARC-08`).
- `handoff/fds.rs` holds the only `unsafe` code (`ARC-12`).

## Side Effects

- Creates the private runtime directory (sockets, lock, log) and the data
  directory (SQLite store, mirrors, timelines, plugins).
- Listens on two Unix sockets: clients and hooks.
- Spawns PTYs, agents, git, `gh`, `glab`, `docker`, `security` and plugin
  processes.
- Network: MCP servers, Jira, Linear and Bitbucket APIs, OAuth endpoints.
- Reads and writes the macOS Keychain.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon`
- End to end: `cargo nextest run -p muster --test <area>` for the area touched
  (`sessions`, `runs`, `worktrees`, `hooks`, `plugins`, `reload`, …)
