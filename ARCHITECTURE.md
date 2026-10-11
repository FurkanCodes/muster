# Muster — architecture

Draft for milestones 1–2. What Muster does and why lives in [PRODUCT.md](PRODUCT.md) (`PRD-*`). This file says how it is built (`ARC-*`). The study of Nebula that informed it is in [docs/research/nebula-study.md](docs/research/nebula-study.md); Nebula is MIT-licensed, and if any of its code is ever copied, its notice goes into `NOTICE`.

## 1. Shape

```
┌──────────────────────────────── host ────────────────────────────────────┐
│  muster (TUI) ─┐                                                          │
│  muster run …  ├── Unix socket, 0700 ──┐                                  │
│                                        ▼                                  │
│  ┌──────────────────────────── muster daemon ──────────────────────────┐  │
│  │ Supervisor: owns every session and run, calls the pure machines     │  │
│  │ Store (SQLite, own thread) · Scheduler · Sampler · Sources · GitHost│  │
│  │ Hook receiver (Unix socket, per-session tokens)                     │  │
│  └───────┬──────────────────────────────────────────┬──────────────────┘  │
│          │ PTY                                      │ Docker API          │
│          ▼                                          │                     │
│  hands-on sessions in git worktrees                 │                     │
│  (claude, codex … as the user)                      │                     │
└─────────────────────────────────────────────────────┼─────────────────────┘
                                                      ▼
                        containers: hands-on sessions (exec with a TTY)
                        and runs (headless agents, checks by exec)
```

The daemon is the only process that owns agent processes and talks to Docker, GitHub and ticket sources. Clients talk only to the daemon. A container sees only itself.

## 2. Core model

- **Workspace** — where code lives: a `Worktree` (a git worktree on the host) or a `Container` (a container with its own checkout).
- **Session** — one agent process in one workspace. Its **driver** is `Pty` (hands-on: a person can type) or `Headless` (structured JSON output, driven by a run). A session can switch driver: take-over stops the headless process and resumes the same agent session interactively; hand-back does the reverse.
- **Run** — one ticket going through the stages in `PRODUCT.md` §5. A run owns a container workspace and creates sessions in it: a builder, then a reviewer.
- **Action** — *Check*, *Draft PR*, *Agent review*. The run machine chains them; on a hands-on session the user triggers them one at a time, and they execute in that session's workspace.

## 3. Workspace

| Crate | Holds | Rule |
|---|---|---|
| `muster-core` | IDs (ULID newtypes), entities, protocol types, typed error codes, harness descriptor types | No IO (`ARC-01`) |
| `muster-state` | The pure state machines: session status and run stages, each `step(state, event, now) -> (state, Vec<Effect>)` | No IO, no async, injected clock (`ARC-01`) |
| `muster-daemon` | Supervisor, store, IPC server, PTY host, container runtime, hook receiver, scheduler, sampler, sources, git host, harness drivers and parsers | Every external system behind a trait (`ARC-03`) |
| `muster-client` | The async framed transport over the socket, and a client connection that performs the handshake; shared by the daemon, the TUI, the binary and the e2e tests | No business logic |
| `muster-tui` | ratatui client: sessions, needs-you, terminal view, dialogs | Renders state; holds no business logic |
| `muster` | The binary (clap): no args opens the TUI; `daemon`, `run <issue-url>`, `spawn`, `status`, `doctor` | Thin |

Main dependencies: tokio, portable-pty, vt100 + tui-term, bollard (Docker API), rusqlite (bundled), serde + rmp-serde, ratatui + crossterm, axum (hook receiver), clap, octocrab or reqwest (GitHub), keyring (macOS Keychain), sysinfo, tracing, ulid.

## 4. Daemon

- **One owner.** A single Supervisor task owns every session's and run's state and is the only thing that calls the machines. The PTY host, container runtime, hook receiver, sampler, sources and git host are tasks that report events to it over channels and carry out the effects it hands out. No shared mutable maps (`ARC-02`); Nebula's thirteen mutex maps with lock order kept in comments are the thing to avoid.
- **Store on its own thread.** One rusqlite connection (WAL) on a dedicated thread, fed commands over a channel. Async code never blocks on SQLite, and keystrokes never touch it.
- **Write-ahead transitions.** A transition and its reason are committed before its effects run (`ARC-10`). Effects are idempotent: "open draft PR" first looks for an existing PR on the branch; "start container" first looks for a container with the run's label.
- **Lifecycle.** The TUI auto-starts the daemon (`setsid`, no controlling terminal) when the socket does not answer. `muster daemon --foreground` runs it attached for development. Runtime and data directories come from `MUSTER_RUNTIME_DIR` and `MUSTER_DATA_DIR` when set, so every test gets its own.
- **Restart** (`PRD-09`):
  - *Containers* outlive the daemon (`ARC-04`). On boot the daemon lists containers labelled `muster.workspace=<id>`, matches them with SQLite, and re-attaches each stream from the last stored offset. A missing container moves its run to *Needs you* with that reason.
  - *Worktree sessions* die with the daemon, because it holds their PTYs. On boot each one is relaunched with the harness's resume (`claude --resume <session-id>`) in the same worktree. A resume that exits within 10 s falls back to a fresh session with a note, as Nebula does. A resumed session starts finished, and unseen if the restart cut a turn short; its hooks get a new token. Sessions whose harness cannot resume (`shell`), whose resume id was never saved (no turn ran), or whose transcript is gone are recorded as exited with an error and marked unseen.

## 5. Sessions

### Hands-on in a worktree (M1)

- **PTY host:** portable-pty spawns the harness in the worktree as the user, with the user's environment minus variables of any parent agent session. One reader thread per PTY feeds a bounded channel (backpressure); a pump batches output (up to 8 KiB or 5 ms, sent at once after a quiet spell so typed echo is never delayed) into a 1 MiB ring numbered by byte offset and broadcasts it.
- **Attach:** `Attach { session, from_seq }` replays the ring and continues live; a client that falls off the ring gets the whole ring and resets its parser. The forwarder rebinds when a session is relaunched under the same id.
- **Stop:** SIGHUP to the child's process group, then SIGKILL to it after 3 s. A natural exit is told apart from a deliberate stop. Processes that moved to another process group are not reached yet.
- **Exit:** announced only once all output is in the ring, waiting at most 2 s after the process exits, since a background job can hold the terminal open. The terminal's output stays readable after the exit until its worktree is removed.
- **No inherited descriptors:** an agent must not inherit the daemon's sockets or files. macOS cannot create a socket close-on-exec in one step, so a spawn racing a `bind` can leak a listening socket into the child, which then keeps a dead socket accepting connections (seen in the e2e tests). portable-pty closes every descriptor above stderr in the child before `exec`; a test asks `lsof` what an idle shell holds.
- **Worktrees:** created with `git worktree add` under `~/.muster/worktrees/<repository>/<branch>` (`MUSTER_WORKTREES_DIR` overrides it), on the branch the user names — checked out if it exists, created from the current commit if not — or on `muster/<six random characters>`. They stay out of the data directory because on macOS its path contains a space, which some build scripts mishandle. Removing a worktree deletes its directory and keeps its branch; uncommitted changes need `force`, and running sessions block it. The worktree's metadata in the repo's `.git` is git's own mechanism; Muster writes nothing else into the user's repository (`ARC-05`).
- **Git:** run without a shell, with a 2-minute limit and without inherited `GIT_DIR`-style variables. It runs while the Supervisor waits, so a slow checkout delays other commands and status updates, though never keystrokes; moving it off the Supervisor is a later step if it hurts.

### Hands-on in a container (M2)

A Docker `exec` with a TTY, resized through the exec-resize call, feeding the same ring and attach protocol. Stopping is `docker stop`, which reaches every process through the container's cgroup.

### Status

- **Signals:** harness hooks, the terminal progress sequence (OSC 9;4), and process exit for hands-on sessions; the JSON output stream for headless ones (a finished turn is a `result` event).
- **Hooks without editing config** (`PRD-13`, `ARC-05`): Claude Code takes extra settings at launch through `--settings <json>`, so its hooks are passed on the command line and never written to disk. Each hook runs `curl --unix-socket <runtime>/hooks.sock` with the session's own token from its environment, times out after 3 s, and never fails the agent. The receiver (axum on that socket) answers at once and only queues the payload; payload fields are read one by one, so a changed field loses only itself. Verified against Claude Code 2.1.291: inline `--settings` hooks run, `--session-id` is honoured, and the fields read are present. A harness without a launch-time option runs without hooks, and its status comes from the progress sequence and process state.
- **Terminal questions:** programs ask their terminal things at startup and wait for answers; Claude Code 2.1.291 asks for keyboard-protocol support, device attributes and the terminal version. The terminal actor answers them, plus device status and cursor position (from a screen built only when first asked), and tracks kitty keyboard flags and bracketed paste for clients. Adapted from Nebula, which leaves the version query unanswered.
- **Identity** (`ARC-11`): every session gets its own random token, and the receiver derives the session from the token. Nebula shares one token per boot and trusts the session id in the query string; Muster does not.
- **Machine:** pure, with an injected clock and exhaustive unit tests (Nebula's best pattern). States: `Fresh` (launched, nothing asked yet), `Running`, `NeedsYou` (a permission prompt or question; several can be open at once), `Finished`, `Exited`, plus an `unseen` flag. The machine reports its next deadline, so the daemon wakes it exactly when a grace period ends instead of polling. It keeps Nebula's hard-won rules: ignore hooks from a different agent session in the same directory; hold `Running` while subagents still work; clear a permission wait only on a tool event from the one who raised it. It drops the screen-scraping and timing tricks.

## 5a. Terminal UI

- **Shape:** `model` mirrors the daemon's state from the snapshot and deltas; `app` is a state machine with no IO that turns keys, pastes and frames into frames to send; `view` draws it with ratatui; `runtime` owns the person's terminal and the link to the daemon. So every behaviour is a unit test, drawn into ratatui's test buffer.
- **Link:** `muster` with no arguments connects, and if nothing answers it starts `muster daemon --detach` (which calls `setsid`, so closing the terminal leaves it running) and logs to `daemon.log` in the runtime directory. A lost connection is shown and retried every half second; on reconnect the UI re-subscribes and re-attaches from the last byte it has. The client connection splits into a sender and a receiver for this.
- **Sessions:** the selected session streams into a preview; ⏎ opens it full screen, where keys go to the program and Ctrl-] comes back (sent as byte 0x1d by terminals without the kitty protocol, which crossterm reports as Ctrl-5, so both are accepted). Keys are encoded from the program's modes — kitty flags, bracketed paste, application cursor keys — which the daemon sends with every attach and on change (`Modes`), since a client parsing output alone loses them once the ring has wrapped. The person's terminal gets the kitty protocol when it supports it, so Shift+Enter reaches Claude Code.
- **Needs you:** questions first, then unseen news, oldest first. Looking at a session clears it, so the selected entry holds its place until the selection moves on.
- **Replies and deltas race:** a reply ("worktree created") can arrive before the delta that adds the worktree. The app remembers what a reply named and selects or opens it once it appears.

## 6. Protocol

Copied from Nebula where it is good, fixed where it is not.

- **Transport and framing:** Unix socket `<runtime>/daemon.sock` with mode 0700. Frames are `[u32 LE length][MessagePack]`, capped at 4 MiB and checked before allocating.
- **Handshake:** `Hello { min_version, max_version, capabilities }`, answered with the agreed version or `Incompatible`. Encoded with named fields (`rmp_serde::to_vec_named`) so the handshake can grow; Nebula's positional encoding froze it.
- **Planes,** each its own enum rather than one 45-variant request type:
  - *State:* `Subscribe` returns a snapshot, then streams `Upserted`, `Removed`, `StatusChanged` and `Transitioned`. The Supervisor takes the snapshot and opens the subscription in the same step, so no delta is missed or repeated — simpler than Nebula's subscribe-before-snapshot ordering, which a single owner makes unnecessary. A subscriber that falls behind gets a fresh snapshot instead of the lost deltas.
  - *Stream:* `Attach { session, stream, from_seq }`, `Input`, `Resize`, `Detach`. Streams are a PTY's terminal output, a headless agent's raw JSONL and parsed events, and check output.
  - *Commands:* carry a `req_id` and get `Ack`, a typed reply, or `Error { code, message }` with a machine-readable code (`ARC-07`).
- **One writer task per connection,** so replies and stream frames never interleave.
- **Clients reconnect** with backoff, re-subscribe, and re-attach from their last sequence number. Nebula's TUI does not.
- **Versioning** (`ARC-09`): every schema change bumps the version; new optional fields use `#[serde(default)]`; a new variant is sent only to peers that announced support; removals bump the minimum.

## 7. Harnesses

A harness is **data plus a parser**, following Nebula's descriptor idea.

- **Descriptor:** id, program, interactive arguments, headless arguments, session-id and resume templates (`--session-id <uuid>`, `--resume <uuid>` for Claude Code, so Muster chooses the id up front and never has to discover it), model and permission-mode flags, how hooks are passed at launch (or none), headless output parser id, credential kind for containers, image install recipe, and which home directories a container keeps on a volume. Users override fields in their config; contradictory entries are rejected at load.
- **Parser:** turns a harness's JSONL into one `AgentEvent` type: `SessionStarted`, `Text`, `ToolUse { name }`, `ToolResult`, `Usage { tokens, cost }`, `TurnFinished { outcome }`, `Error`. M2 implements Claude Code's `stream-json`; Codex's `exec --json` follows in M4.
- **Take-over:** stop the headless process, then start the interactive CLI with `--resume <session-id>` in the same workspace. In a container, `~/.claude` (or its equivalent) lives on a per-workspace volume so the session survives.
- **Roles in a run:** the builder gets the ticket snapshot, the project's instructions and feedback from earlier attempts. The reviewer gets the ticket and the diff with write tools disabled, and must end with a JSON verdict (`approve` or `request_changes`, with file and line comments). The daemon validates it, posts it as the PR review itself, and retries once on invalid output before *Needs you*. Neither role needs GitHub access.

## 8. Runs and containers (M2)

- **Run machine:** `Queued → Provisioning → Building → Checking → OpeningPr → AgentReview → ReadyForYou → Merged`, plus `NeedsYou` and `Cancelled`. Events: `SlotGranted`, `ContainerReady`, `AgentFinished`, `ChecksFinished`, `PrOpened`, `ReviewSubmitted`, `HumanApproved`, `HumanRequestedChanges`, `Merged`, `LimitHit`, `InfraFailed`, and the user's `Cancel`, `Retry`, `TakeOver`, `HandBack`. A failed check, a reviewer's requested changes and a person's requested changes each count one attempt (`PRD-02`, `PRD-12`). Pure and exhaustively tested, including the odd orders: a review arriving after a cancel, an approval while checks re-run, a limit hit during review.
- **Runtime:** Docker Engine API through bollard, behind a `ContainerRuntime` trait with an in-memory fake. Docker Desktop, OrbStack and Colima all expose it on macOS.
- **Image:** per project, from `.muster/Dockerfile` in the repo, or the default `muster-base` image (git, curl, tini, Node, the harness CLIs). Lamphaus adds a JDK and the Android command-line SDK.
- **Checkout without host write access** (`PRD-04`): the daemon keeps a bare mirror of each repo under its data directory and fetches before each run. The mirror is mounted **read-only**; the container clones it into `/work` on its own filesystem.
- **Commits leave as a bundle** (`ARC-06`): the daemon runs `git bundle create - <branch>` through `exec`, fetches the bundle into the host mirror, and pushes from the host with the user's own GitHub credentials. No git-host credential ever enters a container, and the agent cannot push.
- **Processes:** the container's main process is `tini` + `sleep infinity`; agents and checks are separate `exec`s, so the container survives between stages and a person can take over at any time.
- **Limits:** the container's hard memory limit equals the project's reservation. An out-of-memory kill becomes `InfraFailed { reason: "out of memory" }` with a hint to raise the reservation.
- **Caches:** one named volume per project for build caches (Gradle, npm), shared by that project's containers; the user can clear it from the TUI.
- **Actions by hand:** *Check* runs the project's commands in the session's workspace (a host process in the worktree, or an `exec` in the container). *Draft PR* pushes the session's branch (from the worktree with the user's git, or by bundle from a container) and opens a draft. *Agent review* starts a headless reviewer session in the same workspace. *Hand to autopilot* creates a run that starts at `Checking` with the session's branch.

## 9. Scheduler and resources

- **Two pools on macOS.** Worktree sessions use the host's memory; containers use the container runtime's VM. They are measured separately.
- **Sampling:** sysinfo for each worktree session's process tree and for the host; Docker's stats stream for each container (CPU %, memory used and limit). Every 2 s into an in-memory ring for the live view, downsampled into SQLite every 30 s for history.
- **Admission (M3):** the run budget is the container runtime's total memory (from `docker info`) minus a headroom, default 2 GB. Runs start first in, first out, only when active reservations plus their own fit (`PRD-05`). The queue entry carries a reason the TUI shows as it is, for example *"Waiting for 6 GB — 10.5 of 12 GB reserved by #41 and #43."* Hands-on sessions are never queued; a hands-on container session still counts its reservation against the budget.

## 10. Sources and git host

- **`TicketSource` trait:** list with a filter, get, comment; Jira adds status write-back in M5.
- **GitHub source:** REST through octocrab or reqwest, with the token from `gh auth token`, so the user signs in once with `gh`. Automatic pickup follows `PRD-07`: the configured label must have been applied by an allowed account, checked against the issue's event timeline, not just the label's presence. Starting a hands-on session from an issue passes the issue as context, as Nebula does.
- **`GitHost` trait:** push branch, open draft PR, post review with line comments, mark ready, request reviewer, read PR state.
- **Waiting on people:** PR state is polled every 60 s with ETags, which do not count against GitHub's rate limit when nothing changed. Webhooks need a public endpoint and are out of scope (`PRD-10`).

## 11. Secrets

- **Storage:** macOS Keychain through keyring; nothing secret in config files or SQLite.
- **Worktree sessions:** Muster passes no credentials; the harness uses the user's own sign-in, exactly as in a terminal (`PRD-06`).
- **Containers, first:** the agent credential goes only into the agent `exec`'s environment, never the container's, so check stages do not see it. The agent's own child processes still can — acceptable on the user's own repositories only.
- **Containers, before untrusted tickets:** a host-side proxy. The container gets a per-run token and `ANTHROPIC_BASE_URL` (or the harness's equivalent) pointing at the daemon; the daemon adds the real credential.
- **Redaction:** every stored or forwarded stream passes through a redactor that masks known secret values. An end-to-end test asserts no secret appears in SQLite, logs or streams.

## 12. Configuration

`muster.toml` in the repo, with local overrides under `~/.config/muster/`. Hands-on sessions need none of it; runs and actions do. Example for Lamphaus:

```toml
[project]
base_branch = "main"
dockerfile = ".muster/Dockerfile"
memory = "6g"
cpus = 4

[checks]
commands = ["./gradlew --no-daemon testDebugUnitTest", "./gradlew --no-daemon lint"]
timeout = "30m"

[agents]
builder = "claude"
reviewer = "claude"        # a different harness from M4
instructions = ["AGENTS.md", "RULES.md"]

[limits]
attempts = 3
run_timeout = "90m"
max_cost_usd = 10

[intake]
label = "muster"
allowed_accounts = ["<your GitHub login>"]
```

## 13. Testing

Following Nebula's strategy, which is the best part of its codebase.

- **Unit:** both machines exhaustively; parsers against recorded fixtures; scheduler admission; redactor; config parsing; harness argument building.
- **Fakes:** `FakeRuntime`, `FakeGitHost`, `FakeTicketSource`, a fixed clock, `/bin/sh` standing in for a hands-on agent, and a **fake agent** script that replays recorded `stream-json` fixtures for headless sessions. End-to-end tests need no model and no network.
- **End to end:** the real daemon binary per test, isolated directories through environment variables, an RAII guard that stops it on drop, and a raw protocol client. Hooks are tested by typing `curl` into the fake agent's terminal; the progress signal by typing `printf` escape sequences.
- **Restart:** kill the daemon with a worktree session and a run in flight, start it again, assert both come back.
- **Real Docker:** a suite gated by `MUSTER_DOCKER_TESTS=1`, run in CI on Linux.
- **CI:** `fmt`, `clippy -D warnings`, tests on Ubuntu and macOS, `cargo deny` for licenses and advisories, and a **failing** file-size check (`ARC-08`).

## 14. Rules

- **ARC-01** `muster-core` and `muster-state` do no IO.
- **ARC-02** Each piece of mutable state has one owning task; no shared mutable maps.
- **ARC-03** Docker, ticket sources, the git host, harnesses, PTYs and the clock sit behind traits with fakes.
- **ARC-04** Containers outlive the daemon; the daemon reconciles them on boot.
- **ARC-05** Muster never writes to the user's global agent configuration or to files in their repository; what it needs is passed at launch.
- **ARC-06** Git-host credentials never enter a container; commits leave through `git bundle`.
- **ARC-07** Errors across the protocol carry typed codes.
- **ARC-08** No source file over 1,000 lines; CI fails above it.
- **ARC-09** Protocol changes follow the versioning rules in §6.
- **ARC-10** A transition is stored with its reason before its effects run, and every effect is idempotent.
- **ARC-11** A session's identity comes from its own secret token, never from an id the caller supplies.

## 15. Milestone 1 plan — Cockpit

Steps 1–7 and 9 are done; 6 was built before 5 so sessions run in real worktrees from the start, and 9 before 8 because the UI is what makes Muster usable. Harnesses: `shell` (the person's `$SHELL`) and `claude`. End-to-end tests use a stand-in `claude` on `PATH` and fire the exact hook commands from its `--settings`, so resume and hooks are tested for real, unlike Nebula's command override.

0. **Container spike, alongside the rest:** run `claude -p --output-format stream-json` in a container with `CLAUDE_CODE_OAUTH_TOKEN`; record fixtures for a success, a failure and a long tool-heavy turn; measure memory. Settles the headless question before M2.
1. Workspace, CI gates, core types.
2. Session status machine with exhaustive tests.
3. Store: schema (projects, workspaces, sessions, status changes, resource samples), migrations by `user_version`, migration tests.
4. Daemon skeleton: socket, framing, handshake, subscribe/snapshot/deltas, the end-to-end test harness.
5. PTY host: spawn, ring, attach/input/resize, stop escalation; end-to-end tests with `/bin/sh`.
6. Worktrees: create, list, delete, branch naming.
7. Claude Code descriptor: `--session-id`, hooks through `--settings`, hook receiver with per-session tokens, resume after a daemon restart.
8. GitHub issues: list a project's issues and start a session on one.
9. TUI: sessions view, terminal view (vt100 + tui-term), *Needs you*.
10. Resources for worktree sessions (sysinfo).
11. Dogfood: build Muster with Muster for a week; write down every friction and fix the worst.

M2 then adds §8: the run machine, containers, the GitHub git host, the Claude Code parser, actions by hand, take-over and hand-back.
