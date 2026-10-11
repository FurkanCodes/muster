# Nebula source study (commit a00b760, v0.44.0)

Read-only study of the Nebula workspace. Paths are relative to the repo root
(`crates/...`, `vendor/...`). Line numbers refer to this clone. "Not found"
means I searched for it and did not see it; it does not prove absence.

---

## 1. Crate layout

Workspace: `Cargo.toml` (resolver 2, edition 2021, version 0.44.0, MIT). Members:
`nebula-core`, `nebula-daemon`, `nebula-fuzzy`, `nebula-tui`, `nebula`; `vendor/vt100`
is excluded and wired in through `[patch.crates-io]` (`Cargo.toml:1-12`).

| Crate | Rust LOC | Role | Notable deps |
|---|---|---|---|
| `nebula-core` | ~4.0k | Shared protocol (`protocol.rs`, 788), framing (`codec.rs`, 94), entities, ULID id newtypes (`ids.rs`), harness registry (`harness.rs`, 1,169), paths/env constants, layered settings | serde, rmp-serde 1.3 (lock: 1.3.1), serde_bytes, ulid 3, directories 6, tokio (io-util only) |
| `nebula-daemon` | ~26.1k | PTYs, SQLite, git, hook HTTP receiver, status engine, IPC server, in-place restart | tokio 1 (lock 1.53), portable-pty 0.9, rusqlite 0.40 `bundled`, axum 0.8 (hook receiver), nix 0.31, parking_lot 0.12, tokio-util (CancellationToken), subtle (constant-time compare), rand 0.10, vt100 (patched) |
| `nebula-tui` | ~116.7k (incl. a 21,946-line test module) | ratatui client, attach/render, all GitHub (`gh`) work, git diff/log viewers, modals | ratatui 0.30, crossterm 0.29, tui-term 0.3, vt100, pulldown-cmark, image |
| `nebula` | ~9.6k (≈6.9k are tests) | clap 4 binary: no args → TUI; `daemon`, `reload`, `spawn`, `worktree`, `ask`, `orchestrator …`, `ssh`, `tunnel`, `browser`, `upgrade` (`crates/nebula/src/main.rs:12-150`) | clap 4, tracing-subscriber |
| `nebula-fuzzy` | 427 | Dependency-free matcher, separate crate only so dev builds can optimise it (`crates/nebula-fuzzy/Cargo.toml`) | none |

Answers to the specific "which crate for X" questions:
- **Async runtime:** tokio multi-thread with **2 worker threads** for the daemon (`crates/nebula-daemon/src/lib.rs:59-65`).
- **PTY:** `portable-pty` for spawn, plus `nix` for signals, `fcntl` CLOEXEC and `ioctl(TIOCSWINSZ)` on fds it adopts after exec (`crates/nebula-daemon/src/pty/mod.rs:271-326, 717-791`).
- **SQLite:** `rusqlite` (bundled), one synchronous `Connection` behind a `parking_lot::Mutex` (`store.rs:337-357`). Async code calls it through `spawn_blocking` (`registry.rs:260-269`, `server.rs:1051`).
- **IPC:** tokio `UnixListener`/`UnixStream` with hand-rolled length-prefixed frames (`codec.rs`).
- **Serialization:** MessagePack (`rmp_serde::to_vec`) for IPC and the restart state file. JSON (serde_json) for config, hook configs and hook payloads.
- **Terminal parsing:** `vt100` 0.16.2 (on top of `vte` 0.15), vendored and patched. It is used on **both** sides: the TUI renders with it, and the daemon uses it for cursor-position replies and Cursor's question-dialog detection (`pty/cursor.rs`, `pty/question.rs`).

Build profiles (`Cargo.toml:41-62`): every dependency is built at `opt-level=3` even in dev, because unoptimised vt100 replay took 50–140 ms. Release uses `lto="thin"`, `opt-level="z"`, `codegen-units=1`, and `strip="debuginfo"` (symbols kept so panic backtraces have names).

## 2. Daemon ↔ TUI protocol

**Transport:** a Unix socket at `<runtime_dir>/daemon.sock`. The runtime dir is `$NEBULA_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/nebula`, else `/tmp/nebula-<uid>`. The comments call its 0700 mode "the auth boundary, same model as tmux" (`crates/nebula-core/src/paths.rs:10-57`).

**Framing:** `[u32 LE length][rmp payload]`. `MAX_FRAME_LEN` is 4 MiB and is checked **before** allocating or writing (`codec.rs:16-58`, `protocol.rs:36`). A clean EOF at a frame boundary returns `Ok(None)`.

**Messages:** two big enums. `ClientRequest` is at `protocol.rs:50-511` and `ServerEvent` at `protocol.rs:623-768`. They fall into five planes:
- **Handshake:** `Hello{protocol_version}` gets back `HelloOk{protocol_version, daemon_pid}` or `Incompatible`. Any other request before a successful Hello is answered with `Error` and the connection is closed (`server.rs:96-107, 626-640`).
- **State subscription:** `Subscribe` returns one `Snapshot` (projects, worktrees, agents, terminals, links, pr_seen, ui_state). After that, deltas stream on the same connection: `EntityUpserted`, `EntityRemoved`, `StatusChanged{agent,status,changed_at,unseen}`, `FilesOpened`, `ReviewOpened`.
- **PTY plane:**
  - Client sends `Attach{session, from_seq, cols, rows}`, `Detach`, `Input{bytes}`, `Resize`.
  - Daemon sends `Scrollback{base_seq,data}`, `Output{seq,data}`, `SessionExited`, `KittyFlags`, `AttachRefused`. `Attach` has no req_id, so its error is a separate event that names the session.
- **RPC:** CRUD requests carry a `req_id`. The reply is `Ack{req_id, created: Option<EntityId>}` or `Error{req_id, message: String}` (`server.rs:1176-1196`). Some requests have typed replies instead: `WorktreeEntered`, `SessionList`, `SessionText`, `SessionAnswer`, `Metrics`, `OutputTail`, `OrchestratorList`.
- **Fire-and-forget:** `PrewarmAgent`, `PrewarmWorktreeSessions`, `SaveUiState`, `MarkPrSeen`, `MarkAgentSeen`.

**Server structure** (`server.rs:20-74`):
- Each connection gets one reader loop and **one writer task fed by `mpsc(256)`**, so PTY forwards and RPC replies never interleave mid-frame.
- Slow work (worktree create/delete, PR agent creation, metrics `ps` sweep) is spawned as a task, so the request loop keeps reading Input frames.
- Every SQLite or registry call goes through `spawn_blocking`. That includes every keystroke's `Input` (`server.rs:728-742`).

**Versioning:**
- `PROTOCOL_VERSION=50`, `MIN_COMPATIBLE_PROTOCOL=49`. Compatibility is a range overlap (`protocol.rs:9-33`).
- The rules are in `ARCHITECTURE.md:16-25`:
  - Bump the version on every schema change.
  - New optional fields need `#[serde(default)]`.
  - A new variant must be gated so an older peer is never asked to decode it.
  - Removing or renaming anything bumps both constants.
  - `Hello`/`HelloOk` must never gain fields, because compact MessagePack makes them positional.
- Clients gate newer commands with `require_protocol` (`crates/nebula-tui/src/ipc.rs:300-308`).
- On a version mismatch, the client looks up the daemon pid from the kernel (`SO_PEERCRED`) and explains which side is stale (`ipc.rs:143-200`).

**How a reconnecting client catches up:**
- **Entity state:** `Subscribe` registers on the broadcast channel **before** taking the snapshot, so a change made in between arrives after the Snapshot and is folded in by id (`server.rs:642-658`). The broadcast channel holds 1024 events (`registry.rs:235`). A lagged subscriber gets a fresh full `Snapshot` instead of the lost deltas (`server.rs:1148-1170`; test `lagged_subscription_resyncs_with_a_snapshot`, `server.rs:1208`).
- **Terminal output:** every PTY has a 1 MiB byte ring with monotonic sequence numbers (`pty/ring.rs`; a byte's seq is the count of all bytes ever written before it).
  - `Attach{from_seq}` replays from that seq. If the seq has fallen off the ring, it replays the whole ring and the client resets its parser (`ring.rs:70-87`).
  - The TUI sends `from_seq = next_seq` when it kept a painted screen from an earlier visit (`crates/nebula-tui/src/event_loop/selection.rs:930-946`).
  - The attach task subscribes to the PTY broadcast **before** snapshotting the ring and skips bytes the replay already covered (`attach.rs:43-84, 157-186`).
  - If the forwarder lags on the PTY broadcast, it catches up from the ring as a plain `Output` continuation, or as a `Scrollback` reset when the gap is gone (`attach.rs:217-240`).
  - If the PTY is respawned under the same `SessionRef` (restart or relocation), the forwarder notices through the `session_installs` broadcast and rebinds itself (`attach.rs:1-16, 92-146`).
  - A resize "jiggle" (rows-1, then back) forces a repaint only when the replay may have started mid-frame (`attach.rs:69-82`, `pty/mod.rs:448-458`).
- **Grid previews:** `TailOutput{after_seq}` returns nothing when the ring has not grown since the client last asked (`protocol.rs:496-508`).

**Client side:**
- `connect_or_spawn` tries the socket first. If nothing answers, it spawns `current_exe() daemon` with `setsid` (no controlling tty) and polls for up to 3 s (`ipc.rs:30-108`).
- One-shot CLIs open a fresh connection, send one request with `req_id=1`, and skip broadcasts until the matching Ack/Error arrives (`ipc.rs:14-16, 286-298`).
- When the connection drops, the TUI marks itself `Disconnected` and flashes "daemon connection lost" (`event_loop/loop_run.rs:325-329`). I found no automatic reconnect. After `nebula reload` the user is told to relaunch (`crates/nebula/src/upgrade.rs:51-57`).

## 3. Session lifecycle and PTY ownership

**Spawning:**
- `PtySession::spawn` (`pty/mod.rs:271-326`):
  - Opens a PTY.
  - Sets `TERM=xterm-256color` and `COLORTERM=truecolor`, and drops `NO_COLOR`/`FORCE_COLOR`.
  - Scrubs agent-session variables and sets `NEBULA_AGENT_ID`, `NEBULA_API_URL` and `NEBULA_API_TOKEN` (`registry/spawn.rs:559-574`).
- Agents are launched through `/bin/sh -c 'exec </dev/null; exec "$@"' … $SHELL -l -i -c 'exec </dev/tty; unset …; export TERM…; <cmd>'` (`registry.rs:59-71, 817-876`). Why:
  - The CLI gets the user's real login environment, aliases and PATH.
  - rc files see a non-tty stdin, so auto-exec launchers in them don't fire.
- `main` scrubs Claude Code's own identity variables (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, …) before any thread or child exists (`crates/nebula/src/main.rs:18-21`; list at `crates/nebula-core/src/env.rs:102-116`).
- Before a create spawns anything, the daemon checks the CLI exists with `command -v` through the login shell. The answer is cached and the check has a 5 s timeout (`registry/prewarm.rs:244-279`).

**Reading output** (pipeline at `pty/mod.rs:795-952`):
- A blocking reader **thread per PTY** reads 16 KiB at a time and feeds a **bounded mpsc(64)**. `blocking_send` gives natural backpressure against a child that floods output.
- An async `pump` task coalesces output up to 8 KiB or a 5 ms hold. The hold is measured from the *last flush*, so the echo of a keystroke after a quiet spell goes out immediately (`flush_deadline`, `pty/mod.rs:836-854`).
- Each flush:
  1. Feeds the scanners: kitty/DA1/DSR query replies are written straight back into the PTY (tmux-style, `pty/kitty.rs:1-19`), plus OSC 9;4 progress, OSC 0/2 title, the cloud-id scanner and Cursor's question scanner.
  2. Appends the chunk to the ring.
  3. Broadcasts a `PtyEvent` on a 256-slot channel.

**Surviving TUI exit:** the daemon is a separate `setsid` process that owns every PTY master. Detach only aborts that client's forwarding task (`server.rs:721-726`). The rings keep filling while nobody is attached.

**Exit and cleanup:**
- `watch_for_exit` tells a natural death from a deliberate kill. Deliberate kills remove the registry entry first, so the watcher compares with `Arc::ptr_eq` (`registry/exit.rs:16-85`).
- Only a natural death feeds `SessionEnded` to the status machine.
- A resumed CLI that dies with an error within 10 s of spawn is treated as "session not found". Its id is cleared and it respawns fresh (`registry.rs:44-49`, `registry/spawn.rs:615-678`).
- `PtySession::kill` (`pty/mod.rs:466-553`):
  1. Sweeps `ps` for every process group under the child.
  2. Sends SIGHUP.
  3. A watchdog **thread** waits 3 s, then SIGKILLs every group, including jobs the login shell put in their own group.
- Shutdown runs `kill_all` and unlinks the socket (`lib.rs:382-391`). An exclusive `flock` on the pidfile prevents two daemons (`lifecycle.rs:17-60`).

**Lazy respawn:** `ensure_session` spawns a dead session on Attach. It runs under a `spawn_gate` mutex, so two racing attaches never fork two CLIs (`registry/spawn.rs:271-337`). A restored agent resumes with its stored session id.

**Daemon restart, cold:**
- Rings are memory-only, so scrollback is lost.
- The boot sweep marks `running`/`needs_feedback` rows `disconnected` (`store.rs:800-825`, `lib.rs:107-128`).
- The next attach respawns the CLI with `--resume`.

**Daemon restart, in place** (`handoff.rs:1-18`, `ARCHITECTURE.md:27`):
- The daemon `exec`s the new binary over itself, so the pid is unchanged and every agent is still its child.
- It clears `FD_CLOEXEC` on the PTY masters, the client socket, the hook listener and the pidfile, so they survive the exec.
- Each session's ring, terminal modes and status hints go into a MessagePack state file (`Carried`, `pty/mod.rs:672-697`; state format `VERSION=2`). The new image adopts them with `--adopt`.
- The request travels outside the socket protocol, as a request file plus SIGWINCH, because the asking client is usually the newer build.

**Idle reaper:**
- Every 15 s, sessions in worktrees no client is viewing are killed once `session_idle_timeout` passes (default `"5m"`, `crates/nebula-core/src/settings.rs:47`).
- Spared: running/needs-feedback agents, agents with a **detached job** (a descendant that leads its own session, found via `ps` stat `s`), and run terminals (`registry/sessions.rs:214-300`, `pty/mod.rs:148-169`).

**Prewarm pool:** the daemon boots a CLI while the user is still typing a name. Hooks that fire before the agent row exists are buffered (cap 64) and replayed when the row adopts the session (`registry.rs:110-125`, `registry/spawn.rs:97-107, 211-217`, `registry/status.rs:52-73`).

**Why vt100 was vendored** (`vendor/vt100/Cargo.toml` header, `Cargo.toml:5-9`):
1. Rows scrolled out of a **top-anchored DECSTBM region** on the primary screen go into scrollback; upstream discards them (`vendor/vt100/src/grid.rs:620`). Without this, Codex's ratatui inline viewport left the scrollback empty.
2. **OSC 8 hyperlinks** are kept per cell (`cell.rs:17`, `perform.rs:210`, `screen.rs:62`).
3. There are also "history line" and scrollback-release APIs marked NEBULA PATCH (`screen.rs:134-166`, `grid.rs:16, 203-222`).

The version must stay 0.16.2 so the `[patch]` matches.

## 4. Status detection

**Hook installation** happens on every spawn and is chosen by the harness's dialect (`registry/spawn.rs:413-448`). An install failure only logs. It never blocks the spawn.

| Harness | Where hooks go | Events |
|---|---|---|
| Claude | `<worktree>/.claude/settings.local.json` | `UserPromptSubmit`, `Stop`, `SessionStart`, `PermissionRequest`, `Notification` (unmatched, so both `idle_prompt` and `permission_prompt` arrive), `PreToolUse` (matcher `AskUserQuestion`), `PostToolUse` (unmatched), `SubagentStart`, `SubagentStop` (`hooks/installer.rs:58-82`) |
| Codex | **global** `$CODEX_HOME/hooks.json` (default `~/.codex`) | `UserPromptSubmit`, `Stop`, `SessionStart`, `PermissionRequest`, `SubagentStart/Stop`. No Notification event, no AskUserQuestion (`installer.rs:84-93`) |
| Cursor | `<worktree>/.cursor/hooks.json`, in its own dialect | camelCase events mapped through the `hookEvent` query value; needs `"version":1`; each command must print `{"continue": true}` (`installer.rs:95-106, 159-169, 338-364`). Also `.cursor/rules/nebula-title.mdc` |
| Pi, OpenCode | global TypeScript extension or plugin (`~/.config/opencode/plugins/nebula.ts`) | They map native events onto Claude's event names and POST Claude-shaped payloads (`hooks/opencode_plugin.rs:1-18`) |

Why Codex is global: Codex records its hook-trust approval against the hooks *file path*. A per-worktree file would trigger the approval prompt again in every new worktree, and an unanswered prompt means no hooks at all (`installer.rs:26-35`).

Claude also gets `permissions.allow` entries for `Bash(nebula rename|worktree|spawn|open|sessions|read|ask:*)` (`installer.rs:149-157`).

**Merge strategy** (`installer.rs:16-24`):
- **Merge, never replace.**
- Managed groups are tagged `"_nebulaManaged": true`. Every install strips them (plus untagged strays recognised by command signature) and rebuilds them (`installer.rs:171-204, 325-332, 435-459`).
- A corrupt file, or one whose root is not an object, **aborts** the install rather than overwriting user data (`installer.rs:239-270, 366-381`).
- Writes are atomic (tmp file + rename, `installer.rs:400-408`).
- Each hook is a shell one-liner (`installer.rs:108-142`; pinned verbatim by a test at `installer.rs:500-527`). It:
  - exits 0 unless `NEBULA_AGENT_ID` and `NEBULA_API_URL` are set, so hooks are inert outside Nebula;
  - pipes the hook's stdin payload into `curl -sS -m 3 -X POST -H "Authorization: Bearer $NEBULA_API_TOKEN" --data-binary @- "$NEBULA_API_URL/api/hooks/<cli>?agentId=$NEBULA_AGENT_ID&hookEvent=<Event>"` followed by `|| true`;
  - silences output, except `UserPromptSubmit`, whose response body goes to stdout and so reaches the model's context. That is the auto-title channel.
- **Cleanup:** none. Hooks are re-merged on every spawn and left in place afterwards. I found no uninstall and no `.git/info/exclude` handling.

**Receiver** (`hooks/mod.rs:237-302, 361-430`):
- An axum server on `127.0.0.1:0` (random port) with one route per CLI.
- The bearer token is a **32-byte random hex value per daemon boot**, compared in constant time with `subtle` (`hooks/mod.rs:381-389`).
- The agent id and event name come from the **query string**. The token is daemon-wide, not per agent.
- The payload is permissive: every field optional, unknown fields ignored, a parse failure still returns 200 (`hooks/mod.rs:150-196, 391-395`).
- Field aliases cover Cursor: `conversation_id` → session id, first `workspace_roots` entry → cwd.
- Accepted posts go to an `mpsc(256)` drained by one task (`lib.rs:184-228`). That task:
  1. calls `apply_hook_event`;
  2. records the prompt for the "recent prompts" line;
  3. syncs the session title;
  4. re-homes the agent if its cwd moved;
  5. completes any pending relocation.
- The listener fd and token survive an in-place restart, because live agents already carry them in their environment.

**State machine** (`status.rs`, about 1,990 lines with 50 unit tests):
- It is pure: no I/O, an injected `Instant`, and it returns `Vec<Effect>` (`SetStatus`, `SaveSessionId`) (`status.rs:1-3, 153-157, 328-549`).
- **States** (`crates/nebula-core/src/entities.rs:5-20`):

  | State | Meaning |
  |---|---|
  | `Fresh` | gray, never run |
  | `Running` | yellow, working |
  | `Finished` | green, turn complete |
  | `NeedsFeedback` | red, the **"needs you"** state |
  | `Terminated` | nonzero exit while live |
  | `Disconnected` | daemon restarted under it |

  A separate `unseen` flag marks a finish nobody has looked at yet.
- **Transitions:**
  - **Foreign sessions:** hooks carrying a different CLI session id are ignored (`status.rs:337-355`). The session id is persisted for resume only after a turn has run, so it never points at a transcript that doesn't exist (`status.rs:356-370`).
  - **Prompt:** `UserPromptSubmit` → Running.
  - **Stop:** `Stop` → `end_turn`, which gives Finished unless subagents are tracked. With subagents still tracked it holds Running, and `tick` (every 30 s, `lib.rs:238-253`) promotes the row once the set has stayed empty for `DRAIN_GRACE` = 180 s, or after `SUBAGENT_QUIET_GRACE` = 30 min with no subagent traffic (`status.rs:62-82, 557-589, 639-662`).
  - **Late subagent:** a `SubagentStart` within 30 s of a finish heals the row back to Running (`status.rs:451-468`).
  - **Into NeedsFeedback:** `PermissionRequest`, a `permission_prompt` notification, or `PreToolUse(AskUserQuestion | ask_question | question)` → NeedsFeedback, remembering the origin (foreground or a subagent id).
  - **Out of NeedsFeedback:** the next tool event from the same origin → Running. A question is cleared only by its own `PostToolUse` (`status.rs:397-450, 597-623`).
  - **Late echo:** a `permission_prompt` arriving within 5 s of leaving red is ignored as an echo of a dialog already answered (`status.rs:73-82, 625-631`).
  - **Idle:** `idle_prompt` → Finished, or held while subagents remain (`status.rs:664-704`).
  - **Progress busy:** OSC 9;4 busy → Running, but only from Fresh or Finished.
  - **Progress idle:** OSC 9;4 idle → `end_turn`. This is the only signal after a user cancel. A launch that was given a first prompt swallows exactly one startup `9;4;0` (`status.rs:296-308, 473-508`).
  - **Relocation:** a pending relocation holds every turn-end at Running (`status.rs:706-…`).
  - **Process exit:** `SessionEnded` → Finished on exit 0, Terminated otherwise (`status.rs:523-541`).
- Effects are persisted and broadcast as `StatusChanged` (`registry/status.rs:90-115`).

**OSC 9;4 scanning** (`pty/progress.rs:1-116`):
- A bounded OSC framer buffers at most 24 bytes; longer sequences are skipped without allocating.
- `9;4;0` means idle; any other state counts as busy.
- It reports **edges** only.
- The module header has a table of the raw bytes captured from Claude Code 2.1.241: the permission prompt keeps the bar at `3`, so this signal cannot "green out" a row that is really waiting on the user.
- Progress and question edges count only while the PTY still owns its session (`registry/exit.rs:86-120`).

**Per-harness differences:**
- Cursor has no permission hook and always runs with `--force`, so it never reaches NeedsFeedback through hooks. Its ask-question dialog is detected by **screen-scraping a fixed key-legend string** with vt100 (`pty/question.rs:1-30`).
- Muse, Grok and custom harnesses get no hooks. Their status moves only on OSC 9;4 (if the CLI emits it) and process exit.
- Claude `/rename` fires no hook. Nebula sees the OSC 0/2 window-title change instead (`pty/title.rs:1-14`).
- Claude `/model` is picked up by polling the transcript every 1 s (`lib.rs:323-338`).

## 5. Harness abstraction

`HarnessDescriptor` (`crates/nebula-core/src/harness.rs:154-202`) has these fields:
- `id`, `label`, `program`, `enabled`
- `model{flag, default, models, catalog}`
- `effort{flag | config_flag+config_key, default, efforts, offered}`
- `permissions_flag` (`--yolo`, `--force`)
- `prompt_flag` (OpenCode's `--prompt`)
- `resume{flag | subcommand, cd}`
- `system{append_flag | prepend_to_first_prompt}`
- `hooks` (dialect name: claude/codex/cursor/pi/opencode)
- `relocation_prompt`, `compose_model_effort`

The seven built-ins are a compiled table (`harness.rs:368-572`). Examples:
- **Claude:** `--model`, `--effort`, `--resume`, `--append-system-prompt`, hooks `claude`, and **no** permissions flag.
- **Codex:** effort via `-c model_reasoning_effort=…`, resume as `resume <id> --cd <dir>`, `--yolo`.

**Config (`config.json` → `harnesses` map):**
- Entries are `HarnessOverride` deltas. Nullable fields use `Clearable<T>`, which keeps absent (`Keep`), `null` (`Clear`) and a value (`Set`) apart (`harness.rs:583-687`).
- A new CLI is just a new map entry: no code needed.
- `problem()` rejects contradictory descriptors, for example both a resume flag and a subcommand (`harness.rs:297-352`).
- The `AgentKind` enum stays the identity. Custom harnesses are `AgentKind::Custom` plus a registry id, persisted in `agents.custom_harness` (`entities.rs:48-77`).

**argv building** (`registry.rs:442-541`), in order:
1. resume (flag + id, or subcommand + id [+ `--cd`]);
2. permissions flag;
3. model (Cursor composes `model-effort` into one id);
4. effort flag or `-c key=value`;
5. guidance and the PR/issue rule, as the system-prompt flag or prepended to the first prompt;
6. the starting prompt, positional or via `prompt_flag`.

The `NEBULA_AGENT_CMD` env var replaces the whole argv for tests (`registry.rs:453-460`).

Model and effort are stored on the agent row; `None` (the `"default"` sentinel) means the flag is not passed.

**Resume:**
- The session id is captured from hook payloads (`Effect::SaveSessionId`).
- For Claude, the daemon checks that `<projects>/<slug>/<id>.jsonl` exists before resuming (`registry/spawn.rs:452-476`).
- A resume that dies fast falls back to a fresh spawn (§3).

Every harness is modelled as an **interactive TUI CLI**. There is no headless/JSON output mode in the descriptor (Claude `--cloud` is a special case).

## 6. GitHub integration

GitHub work lives **in the TUI**, through `gh` subprocesses, not in the daemon.

**The `gh` wrapper** (`crates/nebula-tui/src/pull_request.rs:175-208`):
- 20 s timeout, stdin null.
- stdout on success; stderr kept so failures can be classified.

**Branch lookup** (`pull_request.rs:352-380`):
- Runs `gh pr view --json number,url,title,state,isDraft,headRefName,headRefOid,baseRefName,isCrossRepository,headRepositoryOwner,mergeable,statusCheckRollup,comments,reviews`.
- The result is `Lookup::{Found, Absent, Unavailable}` (`pull_request.rs:322-350`). Only the stderr text "no pull requests found" means Absent. Every other failure is Unavailable, which keeps the last known row instead of blanking it.

**Checks:** `statusCheckRollup` folds to Absent/Pending/Passing/Failing, and `mergeable=CONFLICTING` means conflicts (`pull_request.rs:99-142`).

**Other commands:**
- Issues: `gh issue list/view/comment/edit` (`issues.rs:450-528`), refreshed every 2 min (`issues.rs:102`).
- The viewer's login comes from `gh api user` (`pull_request.rs:463`).

**Polling cadences** (`crates/nebula-tui/src/event_loop.rs:143-247`), with an explicit GraphQL budget note (5,000 points per hour):

| What | Interval |
|---|---|
| git status of the selected checkout | 2 s |
| selected worktree's known PR | 15 s |
| worktree with no PR yet | backoff 10 s → 3 min |
| other checkouts | 5 min |
| project's open-PR list | 15 s (backoff 30 s → 10 min) |
| other projects' lists | 5 min |
| PR detail fetch | 300 ms debounce |

**Cache** (`pr_cache.rs:1-37`):
- One JSON document plus one `gh pr diff` file per PR, under the data dir.
- Rewritten atomically at most once per git poll.
- Explicitly "never the source of truth".

**Daemon side:**
- Validates PR and issue URLs and stores them on the agent row.
- Rebuilds the PR/issue "rule" prompt on every spawn (`pr_scope.rs:1-24`).
- Can cut a worktree from `refs/pull/N/head` for a fork's PR.
- `CleanupMergedWorktree` re-checks every safety invariant itself: not the main or default branch, HEAD matches, clean checkout, no live sessions (`registry/projects.rs:498-575`).

**Error handling:** every `gh` failure is an ordinary "couldn't ask". The user gets no error flash.

## 7. Persistence

**Database setup:**
- SQLite file at `<data dir>/nebula.db`. The data dir comes from `directories` (`dev.nebula.nebula`), or `NEBULA_DATA_DIR`.
- Pragmas: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON` (`store.rs:344-357`).
- One connection behind a mutex; the comment says write volume is trivial (`store.rs:1-3`).

**Migrations:**
- A `const MIGRATIONS: &[&str]` holds 29 SQL batches, applied from `PRAGMA user_version` (`store.rs:16-335, 369-388`).
- Each batch runs as `BEGIN; …; PRAGMA user_version=N; COMMIT;` with foreign keys **off** for the window, so table rebuilds don't cascade-delete children.
- Migration 28 is a full data merge and table rebuild.
- Tests apply the first N migrations, insert old-shape data, then upgrade (`store.rs:1453-1806`).

**Current tables:**

| Table | Columns / notes |
|---|---|
| `projects` | id, name, repo_path UNIQUE, sort_order, created_at |
| `worktrees` | id, project_id FK CASCADE, path, branch, is_main, sort_order, created_at, pinned; UNIQUE(project_id, path) |
| `agents` | id, worktree_id FK CASCADE, name, status, archived, archived_at, claude_session_id, sort_order, created_at, status_changed_at, kind, pinned, model, effort, auto_title_pending, unseen, cloud_session_id, pr_url, claude_title, recent_prompts (JSON array), issue_url, custom_harness, role |
| `terminals` | + run_command |
| `ui_state` | single-row JSON blob |
| `links` | |
| `pr_seen` | url, marker, seen_at |

The notes/todos and workspaces tables were dropped by later migrations.

**Not persisted:** scrollback, hook events (no event log), metrics. Transcripts are read from the CLIs' own files.

**Settings** (`crates/nebula-core/src/settings.rs:1-80`):
- `config.json` with `config.local.json` merged over it key by key.
- **Lenient per-key parsing**, so one bad key costs only that key. Writes are tmp + rename.

## 8. Worktree management

**Naming:** `<repo>/../<repo-name>-worktrees/<branch>`, with `/` replaced by `-` (`crates/nebula-daemon/src/git.rs:170-182`).

**Creation** shells out to the `git` CLI on purpose (`git.rs:1-3`). Steps:
1. Fetch `origin` with a 30 s timeout and `kill_on_drop` (`git.rs:295-391`).
2. Run `git worktree add <path> --no-track -b <branch> <origin/HEAD | configured base | --base ref>`.
3. If the branch already exists, check it out instead (`git.rs:262-293`).

All worktree operations are serialized by an async `worktree_ops` mutex (`registry/projects.rs:264-317`).

**Worktree hooks** (`worktree_hooks.rs:1-26, 96-140`), used for ports and routes:
- Configured in **git config**: `nebula.worktreeCreateHook` / `nebula.worktreeDeleteHook`. Never a file inside the checkout, because a cloned repo must not run its own code.
- The value is an executable path, spawned without a shell.
- Arguments are the repo and the worktree path. Env carries `NEBULA_HOOK`, `NEBULA_WORKTREE_BRANCH` and `NEBULA_WORKTREE_ID`.
- 30 s timeout (`NEBULA_HOOK_TIMEOUT_MS` overrides).
- stdout/stderr go to **unlinked temp files, not pipes**. A hook that backgrounds a dev server would otherwise hold the pipe open until the timeout.
- A failure is a warning. It never rolls back the git operation.
- The hook runs under the worktree lock, and the Ack waits for it.

**Deletion** (`projects.rs:457-497`, `git.rs:576-614`):
1. Kill the worktree's sessions.
2. Run `git worktree remove [--force]`. A directory that is already gone triggers `prune` instead. A stale lock left by a killed Claude session is unlocked and the remove retried.
3. Delete the row, broadcast, then run the delete hook.

**External sync:** a 2 s mtime probe over `.git/HEAD` and `.git/worktrees/*/HEAD` gates a full `git worktree list` reconcile, so worktrees created outside Nebula appear (`lib.rs:272-321, 394-453`).

Hook payloads that report a cwd in another worktree re-home the agent row (`registry/lifecycle.rs:291`).

## 9. Orchestrator and agent-to-agent commands

**Mechanism:**
- `nebula spawn/ask/read/sessions/worktree/open/rename/orchestrator …` are **one-shot IPC clients** on the same socket.
- The caller's identity is just `NEBULA_AGENT_ID` read from the environment (`crates/nebula-tui/src/ipc.rs:257-273, 562`).

**Gating:**
- The real boundary is "same UID can open the 0700 socket".
- Worker commands check that the caller row exists and isn't archived (`sibling.rs:69-74`).
- Orchestrator commands additionally require the persisted row's `role == Orchestrator` (`orchestrator.rs:41-53`). The module header says prompt text is not part of the trust boundary (`orchestrator.rs:1-5`).
- But the caller id is **asserted by the client**, and `role` is a plain field of `CreateAgent` that any client can set (`protocol.rs:176-179`). I found no daemon-side check that only one orchestrator exists.
- The hook token is shared, so any agent can post hooks claiming another agent's id.

**`ask`** (`session_context.rs:136-211`):
1. Subscribe to status broadcasts.
2. Wait for the target to be idle (unless `--no-wait`).
3. Write a labelled question: a bracketed paste if it has line breaks, then `\r` as a separate write.
4. Wait for that turn to end.
5. Return the ANSI-stripped ring bytes written since the question went in.

**`read`:** prefers the Claude transcript JSONL and falls back to the cleaned ring.

**How agents learn the commands:** through `--append-system-prompt` guidance text (`sibling.rs:15-40`, `orchestrator.rs:18-30`). Claude is pre-allowed in `permissions.allow`; Codex and Cursor rely on `--yolo`/`--force`.

## 10. Testing strategy

**Unit tests** (counts: core 39, daemon 335, tui 1,305, nebula integration 116):
- Pure cores are tested directly: the status machine with `t0() + Duration`, the ring, the OSC framers, `ps`-table parsers, metrics built from a fixed table.
- Hook command strings are pinned verbatim (`installer.rs:500-527`).
- `Store::open_in_memory` plus `Daemon::new` support in-process server tests (`server.rs:1200+`).
- TUI "INPUT PARITY" tests build the app twice, choose a row once by key and once by mouse, and compare state (`ARCHITECTURE.md:110`, `event_loop/tests/mod.rs:3413`).

**`e2e_pty.rs`** (5,397 lines):
- `TestEnv` makes a tempdir and runs the **real binary** (`env!("CARGO_BIN_EXE_nebula") daemon --foreground`) with:
  - `NEBULA_RUNTIME_DIR` and `NEBULA_DATA_DIR` pointing into the tempdir;
  - `SHELL=/bin/sh`;
  - `NEBULA_AGENT_CMD=/bin/sh` (the "agent" is a shell);
  - `NEBULA_WORKTREE_SYNC_MS=100` (`e2e_pty.rs:23-115`).
- Loop periods are env-tunable with a 50 ms floor (`lib.rs:27-45`).
- `DaemonProc` is an RAII guard: SIGTERM, wait 2 s, then kill, so a panicking test never leaks a daemon (`e2e_pty.rs:117-167`).
- Tests talk the raw protocol through `nebula_core::codec` and use `read_events_until` / `read_events_for` helpers (`e2e_pty.rs:169-238`).
- **Hooks are driven from inside the PTY.** The test types a `curl … $NEBULA_API_TOKEN … $NEBULA_AGENT_ID` command into the agent shell, which proves the env injection end to end (`e2e_pty.rs:845-960`).
- **The progress test** types `printf '\033]9;4;3;\007'`. The tty echo of the command text must not trigger the status change; only the real escape bytes may (`e2e_pty.rs:1083-1150`).
- A "blind shell" simulates a missing CLI (`e2e_pty.rs:89-99`).
- Other scenarios: restart rebinding, Codex hooks, reload/upgrade keeping sessions alive, prewarm adoption, idle reaping, a SIGHUP-ignoring agent getting SIGKILLed, external worktree adoption, `ask` round trip and timeout.

**`e2e_tui.rs`** runs the TUI in a PTY and parses frames with vt100. It puts a stub `gh` on PATH and keeps runtime paths short under `/tmp` because of the socket path length limit (SUN_LEN) (`e2e_tui.rs:1-80`).

**CI** (`.github/workflows/ci.yml`), on ubuntu and macOS:
- `cargo fmt --check`
- `clippy --all-targets --locked -D warnings`
- a Python step that **warns** on files over 3,000 non-test lines (`continue-on-error`)
- `cargo build` and `cargo test --workspace --locked`

**Release** (`release.yml`): a tag `v*` builds four targets (darwin aarch64/x86_64, linux-musl x86_64/aarch64) with `--release --locked`, tars them and publishes a GitHub Release. `workflow_dispatch` runs a dry build.

**Makefile** `dev`: an isolated instance with its own runtime and data dirs, seeded from the real DB minus live agent/terminal rows. `make perf` runs an input-latency probe.

## 11. Patterns to copy and weaknesses to avoid

**Copy:**
1. **One writer task per connection; subscribe before snapshot; snapshot again on lag** (`server.rs:46-57, 642-658, 1148-1170`). It is simple and correct.
2. **Seq-numbered byte ring + `from_seq` re-attach + forwarder that rebinds on respawn** (`ring.rs`, `attach.rs`). Use it for any stream: PTY output or JSONL events.
3. **Pure status machine:** injected clock, `Effect` list, exhaustive unit tests, with every heuristic written down next to the CLI version it was verified against (`status.rs`).
4. **Fail-soft, idempotent config injection:** tag managed entries, strip and rebuild, abort on corrupt input, atomic writes, env-guarded commands, 3 s curl timeout, receiver always answers 200 (`installer.rs`, `hooks/mod.rs`).
5. **Harness as data** with `Clearable` overrides and `problem()` validation (`harness.rs`).
6. **Process hygiene:**
   - reader thread + bounded channel for backpressure;
   - latency-aware coalescing;
   - SIGHUP, then SIGKILL every process group;
   - `Arc::ptr_eq` to tell a natural exit from a deliberate kill;
   - resume-failure fallback.
7. **Test seams through env vars** (runtime dir, data dir, agent cmd, tick periods) plus an RAII daemon guard and raw-protocol e2e tests.
8. **Lenient per-key settings parsing**, and **scrubbing the parent CLI's session env**.
9. **External hooks:** no shell, temp-file output, timeout, report but never roll back (`worktree_hooks.rs`).
10. **Explicit protocol-evolution rules** and frame-size checks before allocation.

**Avoid:**
1. **Giant files.** `nebula-tui/src/event_loop/launcher.rs` is 12,851 lines, `event_loop/tests/mod.rs` 21,946, `app.rs` 6,642, `ui/launcher_view.rs` 5,694, `config.rs` 5,494, and the daemon's `registry.rs` 4,448. CI only *warns* at 3,000 lines. Set a hard limit from day one.
2. **Weak identity.** The hook token is per boot and shared by all agents, with the agent id in the query. IPC callers assert their own id. The privileged orchestrator role can be set by any client. Issue a per-agent secret and derive identity from it.
3. **Side effects on the user's global config** (`~/.codex/hooks.json`, `~/.config/opencode/plugins/`) and managed files left in worktrees with no uninstall.
4. **Status heuristics that depend on undocumented CLI internals:** Claude's 6 s deferred notification, Ink legend text scraped from the screen, title glyphs, the progress-bar setting. They are timing-heavy and break when a CLI updates.
5. **String-only errors** over IPC (`Error{message}`): the client cannot branch on them.
6. **A monolithic request/response enum** (~45 request variants) with positional MessagePack, which freezes `Hello` and needs careful gating rules.
7. **About 13 independent mutex maps** in `Daemon`, with lock order documented only in prose (`registry.rs:139-155`). An actor or single-owner state task would be simpler to reason about.
8. **`spawn_blocking` for every keystroke** (`server.rs:728-742`). Metrics and process trees come from shelling out to `ps`, with no CPU figures.
9. **Business logic in the client** (GitHub polling, PR cache). That is fine for an interactive tool and wrong for an unattended pipeline.
10. **No client auto-reconnect**, and a cold daemon restart loses all scrollback.

## 12. Reuse map for a container-based headless pipeline

| Nebula component | Verdict | What changes with Docker and headless agents |
|---|---|---|
| Daemon + TUI as separate processes, auto-spawned daemon, 0700 Unix socket | **Copy** | Same. Keep a `--foreground` mode and env-overridable runtime/data dirs for tests. |
| Length-prefixed MessagePack framing, req_id Ack/Error, version range handshake | **Adapt** | Copy the framing and frame cap. Add **typed error codes**, a capabilities list in the handshake instead of positional `Hello`, and possibly split the enum per plane. |
| Subscribe → Snapshot → deltas, snapshot on lag | **Copy** | Entities become ticket/run/stage/container. Add resource samples as a delta stream. |
| Seq byte ring + `from_seq` replay + rebind on respawn | **Copy** | Feed it from the container's attach/log stream (for example bollard `attach`/`logs` with `follow`). Keep two rings per run: raw stdout (JSONL) and parsed events. Persist the event log in SQLite so a daemon restart doesn't lose history, unlike Nebula. |
| PTY ownership, reader thread, kill escalation, ps group sweeps | **Mostly not needed / adapt** | Headless agents (`claude -p --output-format stream-json`, `codex exec --json`) need no PTY. For human takeover, create a Docker **exec** with `Tty=true` and a hijacked stream, resize through the exec-resize endpoint, and reuse the ring and attach protocol. Killing is `docker stop` (SIGTERM to PID 1, so run with `--init`/tini) then `docker kill`: cgroups replace the process-group sweep. The kitty/DA/DSR query answering matters only in takeover mode. |
| In-place restart (fd handoff) | **Not needed** | Containers outlive the daemon. Store container ids and labels in SQLite; on boot, reconcile with `docker ps --filter label=<app>.run=<id>` and re-attach logs from a timestamp or offset. This is strictly better than Nebula's `Disconnected` sweep. |
| Hook injection + loopback HTTP receiver + per-boot token | **Adapt** | Headless stream-json already says when a turn ends (`result` event) and exposes tool use, so hooks become optional (useful for `PermissionRequest`/`Notification` if you need them). Crossing the container boundary: **bind-mount a per-run Unix socket** into the container and use `curl --unix-socket` (no network exposure). The alternative is the host gateway (`host.docker.internal`, which needs `--add-host=host.docker.internal:host-gateway` on Linux) plus a firewall. Use a **per-run token** and derive the run id from the token, never from the query. Write hook config into the container's own `$HOME` or image (no merge with user files needed), but keep the managed-tag merge for a repo's committed `.claude/settings.json`. The image must contain curl. |
| Status state machine | **Adapt (copy the pattern)** | Keep the pure machine with an injected clock and `Effect`s. The state set becomes pipeline stages: Queued, Provisioning, Running, NeedsInput, Testing, PR open, Reviewing, AwaitingApproval, Done, Failed. Per-turn agent status comes from parsed JSON events. Keep the foreign-session gating idea, the subagent drain hold, and OSC 9;4 only for takeover sessions. Drop the screen scraping. |
| Harness descriptor + overrides | **Adapt** | Add `headless_args`, an `output_format` parser id (claude-stream-json / codex-exec-json / opencode), `interactive_args` for takeover, `resume` (takeover needs `claude --resume <session_id>` in the same container after the headless process exits; keep `~/.claude` on a volume), a permission mode, credential mounts, image and resource limits. |
| GitHub via `gh` in the client | **Adapt, move to daemon** | The pipeline must run unattended, so issues, PRs and checks belong in the daemon (gh CLI or octocrab). Copy the `Found/Absent/Unavailable` lookup, the checks/mergeable folding and the budget-aware backoff, and prefer webhooks later. Use a scoped token inside containers for `git push`/`gh pr create`, never the daemon's. |
| SQLite + `user_version` migration list + migration tests | **Copy** | Add tables: tickets, runs, stages, containers, append-only events, resource samples. Consider a dedicated DB thread or actor instead of a mutex plus `spawn_blocking`. |
| Git worktrees + worktree hooks | **Adapt** | Containers need a checkout. Either clone into a per-run volume, or bind-mount a host worktree. In the bind-mount case the worktree's `.git` *file* points at an absolute host path, which breaks inside the container unless it is mounted at the same path. Reuse the naming scheme, the git-config-keyed lifecycle hook and the "report, don't roll back" rule as container create/delete hooks (ports, routes, compose projects). |
| Orchestrator / `spawn` / `ask` | **Not needed for v1** | If agents need a callback ("ask a human", "request a review"), expose a narrow per-run authenticated endpoint over the same mounted socket. |
| Metrics (`ps` RSS sweep) | **Replace** | Use the Docker stats API (CPU %, memory usage and limit per container) and host free memory for **admission control**. Nebula has no CPU readings and no admission. |
| Idle reaper, prewarm pool | **Adapt later** | They become run timeouts and an optional warm-container pool. |
| Test strategy | **Copy** | Real daemon binary per test, isolated dirs through env, RAII guard, raw-protocol client. Replace `NEBULA_AGENT_CMD=/bin/sh` with a **fake agent script** that replays stream-json fixtures. Put Docker behind a trait with a fake for unit and e2e tests, and run a gated suite against real Docker in CI. Copy the fmt/clippy `-D warnings`/test gates and the musl/darwin release matrix, and make the file-size check fail rather than warn. |
