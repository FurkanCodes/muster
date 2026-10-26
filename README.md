<div align="center">

<img src="apps/desktop/src-tauri/icons/128x128@2x.png" alt="Muster's logo: Kep, a pixel-art sheepdog" width="128" height="128">

# Muster

**One place to run all your coding agents.**

<details open>
<summary><b>Desktop app</b></summary>
<br>
<img src="docs/media/muster.gif" alt="Muster's desktop app: agents in one list, a new one started in its own worktree, one that needs you answered with a click, an agent's turn put back, a ticket handed to an agent, and muster ssh to a server" width="100%">
</details>

<details>
<summary><b>Terminal UI</b></summary>
<br>
<img src="docs/media/muster-tui.gif" alt="Muster's terminal UI: agents in one list, a worktree made and an agent started in it, one that needs you answered from the queue, its turn put back, and a ticket handed to an agent" width="100%">
</details>

Run Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI and plain shells side by side, each in its own git worktree, and see at a glance which one is working, which has finished, and which is waiting on you.

[![CI](https://github.com/FurkanCodes/muster/actions/workflows/ci.yml/badge.svg)](https://github.com/FurkanCodes/muster/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/FurkanCodes/muster)](https://github.com/FurkanCodes/muster/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

[Install](#install) · [Quick start](#quick-start) · [Features](#features) · [Servers & VMs](#agents-on-your-servers-and-vms) · [Guide](docs/guide.md) · [Plugins](#plugins) · [Development](#development)

</div>

---

## Why Muster

Running several coding agents at once gets messy fast. They share one checkout, their terminals get lost in tabs, and you only find out an agent was waiting for your answer ten minutes later.

Muster fixes that:

- **Every agent gets its own worktree**, so their changes never collide.
- **One list shows them all**, with a live status: working, finished, or *needs you*.
- **Sessions live in a background daemon.** Close the window and nothing stops. After a restart, agents pick up their conversations again.
- **Run them on a server or VM** over plain ssh, so they keep working while your laptop sleeps.
- **Muster tells you when something needs you**, with a notification, a sound, or the MacBook notch.

It works in your terminal (a full terminal UI), in a Mac app, and from your shell as plain commands. All three are views over the same sessions.

## Features

| | |
|---|---|
| **Agents in worktrees** | Start any agent or shell in a fresh git worktree (`~/.muster/worktrees/<repo>/<branch>`), or in the project itself. Plain folders work too. |
| **Live status** | Running, finished or *needs you*, plus what each agent is doing right now (`editing view.rs`, `running cargo`). Read from the agents' own hooks, never by scraping the screen. |
| **Needs you** | One queue of every question and every finished turn, oldest first. Mac notifications with sounds of your choice, and a click jumps to the session. |
| **Real terminals** | Each session is a real terminal beside the list, or full screen. Arrange panels side by side, split them, and drag them where you like. |
| **Survives restarts** | Sessions belong to the daemon. Quit the UI or the terminal and they keep running; after a daemon restart, agents resume their conversations. |
| **Tickets** | Open issues from GitHub, GitLab, Jira or Linear. Assign an agent to a ticket and it starts in its own branch with the ticket as its first prompt. |
| **Changes and undo** | See what each worktree changed (`+120 −30`) and every agent turn's diff. Undo a turn and the files go back as they were. |
| **Collisions** | Two worktrees touching the same files are flagged as `overlap`, or as `conflict` where git says a merge would fail. |
| **Pull requests** | Each branch shows its PR's state (`#42 approved ✓`). Run checks, have Claude Code review a change, or push and open a draft PR on one key. |
| **Tidy and stack** | Have an agent rewrite unpushed commits into clean ones, or build a worktree on top of another branch that hasn't merged yet. |
| **Servers and VMs** | `muster ssh` sets Muster up on a server or VM over plain ssh, so agents keep working while your laptop sleeps. [More below](#agents-on-your-servers-and-vms). |
| **The notch** | On a MacBook, the notch shows how many agents are working and waiting. Point at it to see the list. |
| **Plugins** | Add ticket sources and side panels in any language. |

The [guide](docs/guide.md#features-in-detail) covers each one in full.

## Install

Muster runs on **macOS** (Apple Silicon and Intel) and **Linux** (x86-64 and ARM).

```bash
curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/main/install.sh | sh
```

This installs `muster` into `~/.local/bin`, and on a Mac the Muster app into `/Applications` (leave the app out with `sh -s -- --no-app`). Each download is checked against its published checksum, and also against GitHub's build attestation when the [GitHub CLI](https://cli.github.com) is signed in.

**You'll also want:**

- `git`
- the agents you plan to run (Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI)
- `gh` or `glab`, signed in, for GitHub or GitLab issues and pull requests

**To upgrade,** run `muster upgrade`. Running sessions keep going through the upgrade.

## Quick start

Open the terminal UI. The first run starts the daemon:

```bash
muster
```

1. Press `a` to add a project (any folder or git repository).
2. Press `n` to start an agent or a shell in it, or `w` to make a new worktree first.
3. Press `⏎` to type into the selected session, and **Shift-Q** to come back to the list.
4. Press `h` at any time to see every key on the screen.

The keys you'll use most:

| Key | Does |
|---|---|
| `↑` `↓` | move through projects, worktrees and sessions |
| `⏎` | type in the selected session (or start one) |
| `n` / `w` | new session / new worktree |
| `d` | what the worktree changed, with each agent turn |
| `⇥` | next tab: *Sessions*, *Needs you*, *Tickets*, *Settings* |
| `Shift-T` | a shell beside the agent, in the same worktree |
| `x` | close a session, worktree or project |
| `q` | quit the UI (sessions keep running) |

The mouse works for everything too: click, drag panels, right-click for a menu. You can change any key on the *Settings* tab. The [guide](docs/guide.md#the-terminal-ui) lists every key.

## From your shell

Every action is also a command, for your terminal and your scripts:

```bash
cd ~/code/app
muster claude "Fix the login redirect"     # an agent in a new worktree, in the background
muster codex "Add tests for the session list"

muster list agents                         # what's running and how it's doing
muster wait a1b2c3 && muster logs --reply a1b2c3
muster attach a1b2c3                       # jump into its terminal

muster list tickets                        # the project's open tickets
muster ticket 42 claude                    # an agent on ticket #42
muster pr fix-login                        # push and open a draft PR
```

Add `--json` to `muster status` or `muster list` for machine-readable output. To make a bare `claude` start a Muster session, add `eval "$(muster init zsh)"` to your `~/.zshrc` (or use `bash`, `fish`).

The [guide](docs/guide.md#from-your-shell) has the full command line: workspaces, driving terminals, tickets, and scripting.

## Agents on your servers and VMs

Run your agents on a machine that never sleeps: a cloud server, a VM on your PC, or a Mac at home. It can be macOS or Linux, Intel/AMD or ARM; anything you can `ssh` into works. Your laptop can close, and the agents keep working.

```bash
muster machine add vm you@203.0.113.7   # give the machine a name
muster ssh vm                           # set it up and open a shell there
```

The first `muster ssh` checks the machine. It offers to install Muster there, says what's missing (git, `gh`, Claude Code signed in), and then opens your login shell on it, with `muster` and `claude` ready. Muster uses your own ssh, keys and config, and adds nothing to them. If you forward your ssh agent, agents there push with your keys.

Once it's set up, drive the machine from your laptop:

```bash
muster --on vm                          # your own UI, showing the machine's agents
muster --on vm list agents              # any command, run against the machine
muster --on vm attach fix-login/2       # jump into a session there
muster ssh vm claude auth login         # run one command there (here, sign Claude Code in)
muster ssh vm --install-claude          # install Claude Code there
```

`muster --on vm` reconnects by itself if the link drops. The machine only needs ssh: `muster ssh vm --install` downloads and checks the release on your laptop, then sends it over. See [the guide](docs/guide.md#from-your-shell) for more.

## Tickets and pull requests

Muster works out the host from the project's git remote: GitHub, GitLab or Bitbucket. For Jira or Linear, add a few lines to the project's `muster.toml`:

```toml
[tickets]
tracker = "jira"              # github, gitlab, jira, linear, or a plugin
site = "acme.atlassian.net"
project = "APP"
```

Then sign in once with `muster auth jira` (or `linear`, `bitbucket`). On a Mac, sign-ins are kept in the Keychain. See [Where tickets and pull requests are](docs/guide.md#where-tickets-and-pull-requests-are).

## Plugins

GitHub, GitLab, Jira and Linear are themselves plugins, written against the same protocol yours would use. A new tracker or side panel is a small program away, in any language:

```bash
muster plugin install https://github.com/someone/their-plugin
```

Muster shows everything a plugin would run before you approve it, and runs nothing until you do. [PLUGINS.md](PLUGINS.md) lists the plugins, [docs/plugins.md](docs/plugins.md) explains how to write one, and [examples/plugins](examples/plugins) has examples.

## Security and privacy

- **Local only.** There's no Muster server, account or telemetry.
- **Your agents, your sign-ins.** Agents run as you. Muster never edits your repository or your agents' settings.
- **Tickets are untrusted.** A ticket reaches an agent fenced off as data, not instructions, and pasted credentials are hidden first.
- **A private daemon.** Its sockets are open to your user only.

Found a security problem? Please open a [private security advisory](https://github.com/FurkanCodes/muster/security/advisories/new) rather than a public issue.

## Documentation

| Read | For |
|---|---|
| [docs/guide.md](docs/guide.md) | every feature, key, command and setting |
| [PRODUCT.md](PRODUCT.md) | what Muster is for, and its rules (`PRD-*`) |
| [ARCHITECTURE.md](ARCHITECTURE.md) | how it is built, and the rules the code is held to (`ARC-*`) |
| [docs/plugins.md](docs/plugins.md) | writing a plugin |
| [apps/desktop](apps/README.md) | the Mac app |

---

## Development

Muster is a Rust workspace with one rule at its centre: **the daemon owns everything, and clients only draw it.**

```
 terminal UI ──┐
 Mac app ──────┼──▶ muster daemon ──▶ agents & shells on PTYs, in git worktrees
 CLI / notch ──┘    (one writer, SQLite)
```

| Crate | Role |
|---|---|
| `muster-daemon` | One Supervisor task owns all state and is its only writer; each change is stored in SQLite before it is broadcast. Each terminal is an actor with a sequence-numbered output ring, so a reconnecting client gets exactly what it missed. |
| `muster-state` | Session status as pure state machines with an injected clock; every rule is a unit test. |
| `muster-core` / `muster-client` | The protocol (length-prefixed MessagePack, typed errors, version negotiation) and its transport. |
| `muster-model` | A client's copy of the daemon's state. It is pure, and shared by the terminal UI and the desktop app. |
| `muster-tui` | The terminal UI (ratatui), a state machine with no IO, tested by feeding it events. |
| `muster-notch` | The MacBook notch, drawn through AppKit. |
| `muster` | The binary. |

The desktop app (`apps/desktop`) is Tauri 2 with React 19: a window over the same daemon.

### Build and test

```bash
cargo build
cargo nextest run -p <crate>                              # the tests of what you changed
cargo fmt --check
CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets
```

The toolchain is pinned in `rust-toolchain.toml`. The suite has over 700 tests, including end-to-end tests that run the real binary against real shells on real pseudo-terminals in real git worktrees, with stand-in `claude` and `gh` programs on `PATH`. CI runs formatting, spelling, clippy, `cargo deny`, unused-dependency checks, a build on the oldest supported Rust, and the tests on macOS and Linux. [CLAUDE.md](CLAUDE.md) says which tests to run for which change. `scripts/soak.sh [minutes]` runs a daemon under steady churn to catch leaks.

For the desktop app, run `pnpm install`, then `pnpm lint`, `pnpm typecheck` and `pnpm test` in `apps/desktop`.

### Repository blueprint

Every code directory has a `README.md` contract of its own (its interface, invariants and side effects), rolled up here. Test-only folders, fixtures, assets and generated bindings have none.

**Local interface and invariants**

- `install.sh`: the one-command installer (`curl … | sh`).
  - Downloads the release binary for this machine (`MUSTER_VERSION`, `MUSTER_INSTALL_DIR`).
  - Checks its SHA-256 and, where `gh` is signed in, its build attestation (`MUSTER_SKIP_ATTESTATION=1` takes the checksum alone).
  - Builds from source where no binary exists.
  - On a Mac, installs the Muster app too (`--no-app` or `MUSTER_APP=0` leaves it out; `MUSTER_APP_DIR`).
  - On a terminal, it speaks in colour (`NO_COLOR` for none).
  - A download that fails its checks installs nothing (`ARC-21`).
  - `MUSTER_DOWNLOADS` points it at a stand-in for tests.
- `Cargo.toml`: the workspace of the crates below. `apps/desktop/src-tauri` is a Cargo workspace of its own.

**Subdirectories and system boundaries**

- crates: [./crates/README.md](./crates/README.md): the Rust workspace:
  - `muster-core`: IO-free shared types, protocol 58, strict readers;
  - `muster-state`: pure session and run machines;
  - `muster-model`: a client's copy of state;
  - `muster-client`: transport;
  - `muster-daemon`: the Supervisor and everything it reaches;
  - `muster-tui`, `muster-notch` and the `muster` binary.
- apps: [./apps/README.md](./apps/README.md): the desktop app (Tauri 2 + React 19), a window over the same daemon (`ARC-24`, `ARC-25`, `ARC-26`), and the README's videos, cut from the desktop app's real window and the terminal UI's real screen (`apps/video`).
- scripts: [./scripts/README.md](./scripts/README.md):
  - CI rule checks (`check-*`): file and type size `ARC-08`, secrets `PRD-06`, test sleeps `ARC-30`, TUI colours `ARC-29`, duplication `ARC-31`;
  - agent-session measurement;
  - the Linux build in Docker (`ARC-27`);
  - logo rendering;
  - the daemon soak test;
  - Claude Code hooks.
- docs, examples: no code contracts. [docs/guide.md](docs/guide.md) is the user guide, `docs/media` holds the README's videos (made by `apps/video`), [docs/plugins.md](docs/plugins.md) explains how to write a plugin, and each folder under [examples/plugins](examples/plugins) has a README of its own.

**Global invariants**

- The daemon owns all state. One Supervisor task is its only writer, and every change is stored before it is broadcast (`ARC-02`, `ARC-10`). Clients only draw it (`ARC-24`).
- `muster-core`, `muster-state` and `muster-model` do no IO (`ARC-01`). Docker, git hosts, ticket sources, harnesses, PTYs and the clock are reached only through traits (`ARC-03`).
- Every protocol change raises its version. Frozen handshake samples are added, never edited (`ARC-09`).
- No source file is over 1,000 lines, and no struct has over 40 fields (`ARC-08`). `unsafe` appears only in the daemon's restart hand-off and the notch's AppKit code (`ARC-12`).
- Tasks are owned and every wait has a deadline (`ARC-13`). Queues, rings and logs are bounded (`ARC-14`, `ARC-15`).
- No real credential goes in code, fixtures, logs or output, and credentials pasted into tickets are hidden (`PRD-06`). Ticket and plugin text is data, never instructions, and is stripped of control characters (`PRD-07`).
- Nothing is written into the person's repository or agents' settings (`PRD-13`, `ARC-05`). Plugins run nothing until their manifest's hash is approved (`PRD-15`).

**Global side effects**

- Muster's data folder: the SQLite store with backups, timelines, plugin folders and run mirrors.
- Muster's runtime folder: two private Unix sockets, the daemon's lock and the notch's, the daemon log, and the forwarded ssh-agent link.
- Child processes: agents and shells on PTYs, git, `gh`, `glab`, Docker, plugin programs, ssh.
- Network: only to the person's trackers and git hosts (Jira, Linear, Bitbucket, MCP and OAuth sign-ins), to GitHub releases for upgrades, and to a plugin's repository on install or update.
- The macOS Keychain, for sign-ins and tokens. On a Mac, notifications and the notch panel.

**System verification**

- `cargo fmt --check` and `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets`.
- Tests of what changed: `cargo nextest run -p <crate>` plus the end-to-end files in `crates/muster/tests/` for the area touched (see [CLAUDE.md](CLAUDE.md), *Which tests to run*). Also `pnpm lint`, `pnpm typecheck` and `pnpm test` in `apps/desktop`, and `cargo test` in `apps/desktop/src-tauri`.
- The whole suite (`cargo nextest run --workspace`) runs only for a protocol, dependency or toolchain change, or a release.

## License

[MIT](LICENSE). Contributions are welcome. Read [ARCHITECTURE.md](ARCHITECTURE.md) and the README of the folder you're changing first.
