# Muster

**A terminal cockpit for coding agents.** Run Claude Code and shells across your projects, each in its own git worktree, from one terminal UI — and see at a glance which agent is working, which has finished, and which is waiting on you. Sessions live in a background daemon, so closing the UI stops nothing, and Claude Code sessions resume after the daemon restarts.

> Status: milestone 1 (the cockpit) is built, tested and dogfooded. Milestone 2, the autopilot (a ticket in, a reviewed pull request out, in containers), has its foundations in place. See [PRODUCT.md](PRODUCT.md). No release yet.

## What it does

- **Sessions in worktrees, or in your checkout.** Add a repository, create a worktree per piece of work (`~/.muster/worktrees/<repo>/<branch>`), and start Claude Code or a shell in it, or start one in the repository itself. Removing a worktree keeps its branch and refuses to discard uncommitted work unless you insist. Branches shown follow what is checked out, also when you or an agent switch, and worktrees made or removed with plain git appear and go.
- **Live status, no screen scraping.** Claude Code reports through hooks passed on its command line (`--settings`), never written into your repository or your Claude settings. Muster shows each session as running, finished, or *needs you* (a permission prompt or a question), with a dot for news you have not seen.
- **Needs you.** One queue of everything waiting on you: questions first, then news, oldest first. While your terminal window is in the background, Muster tells you as something comes to need you, with a desktop notification where your terminal shows them (iTerm2, WezTerm, Ghostty, kitty) and its bell elsewhere; `MUSTER_NOTIFY=off` turns that off.
- **A real terminal per session.** Open any session full screen and use it as you would in your own terminal: the daemon answers the program's terminal questions, and keys are encoded the way the program asked (kitty keyboard protocol, so Shift+Enter reaches Claude Code; bracketed paste; application cursor keys).
- **Survives restarts.** Sessions belong to the daemon. After a daemon restart, Claude Code sessions resume with `--resume`; one that cannot falls back to a fresh start.
- **GitHub issues.** Pick an open issue and Muster creates `muster/issue-<n>-<title>`, starts Claude Code there, and hands it the issue — marked as a description of the work, not as instructions, since anyone can file an issue.
- **Resources.** Memory and CPU for each session's whole process tree, and the machine's memory.

## Install and run

Muster runs on macOS and Linux. You need git, Rust 1.95 or newer to build it, and — for the features that use them — Claude Code and the GitHub CLI (`gh`), signed in.

```bash
cargo install --locked --path crates/muster
```

`--locked` builds the exact dependency versions the tests ran against.

```bash
muster
```

`muster` opens the UI and starts the daemon if none is running. Press `a` to add the repository you started it in.

| Key | In the list |
|---|---|
| `↑` `↓` | move (the selected session streams into the preview) |
| `⏎` | open a session full screen; on a worktree or checkout (`⌂`), start a session; on a project, create a worktree |
| `a` | add a project |
| `w` | new worktree for the selected project |
| `n` | new session in the selected worktree, or on a project in its own checkout: Claude Code with an optional first prompt, or a shell (`⇥` switches) |
| `i` | the project's open GitHub issues; `⏎` starts work on one |
| `s` | stop a session |
| `r` | start an ended session again: Claude Code resumes its conversation, a shell starts afresh |
| `x` | remove the selected ended session; or, after asking, the selected worktree, or a project (Muster forgets it; the repository is untouched) |
| `⇥` | switch between all sessions and *Needs you* |
| `q` | quit the UI (sessions keep running) |

In a full-screen session every key goes to the program; **Ctrl-]** comes back to the list.

The mouse works too. The wheel scrolls back through a session's output, or reaches the program if it handles the mouse itself, as Claude Code does. In the list it moves the selection, or scrolls the preview (as do `PgUp`/`PgDn`, with `End` back to the newest output); a click selects a line, and a click on the preview opens it. To select text in your terminal while Muster has the mouse, hold Option (iTerm2, Terminal.app) or Shift (most others) as you drag.

The same things work from scripts:

```bash
muster project add ~/code/app
```

```bash
muster worktree add app --branch fix-login
```

```bash
muster session start fix-login --harness claude --prompt "Fix the login redirect"
```

```bash
muster issue start app 42
```

```bash
muster status
```

```bash
muster session restart <session-id>
```

After upgrading muster, stop the daemon the old build left running; the next `muster` starts the new one, and Claude Code sessions resume:

```bash
muster daemon stop
```

## How it is built

A Rust workspace of six crates around one rule: the daemon owns everything, and clients only draw it.

- **`muster-daemon`** — a single Supervisor task owns all state and is the only writer (no shared mutable maps); every change is stored in SQLite before it is broadcast. Each session's terminal is an actor owning a sequence-numbered output ring, so a client that reconnects gets exactly what it missed. Hooks arrive on a private socket and are matched to sessions by a per-session token. GitHub calls run off the Supervisor's loop.
- **`muster-state`** — the status logic as pure state machines with an injected clock: every rule (subagents still working, several open prompts, a permission notice that arrives late) is a unit test.
- **`muster-core`** / **`muster-client`** — the protocol (length-prefixed MessagePack with named fields, typed error codes, version negotiation) and its transport.
- **`muster-tui`** — ratatui. The app is a state machine with no IO, so behaviour is tested by feeding it events and drawing it into a test buffer.
- **`muster`** — the binary.

[ARCHITECTURE.md](ARCHITECTURE.md) has the details and the rules (`ARC-*`) the code is held to; [PRODUCT.md](PRODUCT.md) has what Muster is for (`PRD-*`).

## Development

```bash
cargo test --workspace
```

The suite (over 350 tests) includes end-to-end tests that run the real binary in throwaway directories: real shells on real pseudo-terminals in real git worktrees, a stand-in `claude` and `gh` on `PATH` (so hooks, resume and issue handling are exercised exactly as in use, without an account or network), and the UI itself driven by keystrokes on a pseudo-terminal. CI runs formatting, clippy with warnings as errors, a 1,000-line file limit, `cargo deny`, and the tests on macOS and Linux.

## Credits

Muster learned from [Nebula](https://github.com/AgentSystemLabs/nebula) and adapts some of its code — terminal query answering, key encoding, Claude Code's hook events and transcript lookup — under the MIT license; see [NOTICE](NOTICE). The study that informed the design is in [docs/research/nebula-study.md](docs/research/nebula-study.md).
