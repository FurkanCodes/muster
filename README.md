# Muster

**A terminal cockpit for coding agents.** Fetch your tickets, assign agents to them, and run Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI and shells across your projects, each in its own git worktree, from one terminal UI — and see at a glance which agent is working, which has finished, and which is waiting on you. Sessions live in a background daemon, so closing the UI stops nothing, and Claude Code sessions resume after the daemon restarts.

> Status: built, tested and used daily by its author. Tickets come from GitHub, GitLab, Jira and Linear; agents are Claude Code, Codex, Gemini CLI, OpenCode and Copilot CLI. An autopilot that took tickets to reviewed pull requests in containers was built and set aside; its code is kept. See [PRODUCT.md](PRODUCT.md). Each release comes with prebuilt binaries for macOS and Linux.

## What it does

- **Sessions in worktrees, or in your checkout.** Add a repository (or any folder), create a worktree per piece of work (`~/.muster/worktrees/<repo>/<branch>`), and start a coding agent (Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI, or [one of your own](#agents-of-your-own)) or a shell in it, or start one in the repository itself. Removing a worktree keeps its branch and refuses to discard uncommitted work unless you insist. Branches shown follow what is checked out, also when you or an agent switch, and worktrees made or removed with plain git appear and go. A plain folder, not a git repository, works too: sessions and checks run in the folder, and worktrees, tickets, reviews and pull requests come once you make it one (`git init`).
- **Live status, no screen scraping.** Each agent reports the way it allows without touching your setup: Claude Code through hooks passed on its command line (`--settings`); Gemini CLI, OpenCode and Copilot CLI through hooks or a plugin in a file of Muster's own that they are pointed at for the session; Codex through its turn notice and its window title; an agent of your own (below) through its terminal's progress signal, its window title and its exit. Nothing is written into your repository or your agents' settings. Muster shows each session as running, finished, or *needs you* (a permission prompt, a question, or an MCP server's form asking for your input), with a dot for news you have not seen. Each agent's line also says what it is doing, as its hooks tell (`editing view.rs`, `running cargo`, `wants to run rm`: a file's name and a command's program, never its arguments), and how long it has run, waited or sat finished; the top bar counts the agents working, waiting and done, beside the machine's memory, and each project has a colour of its own. A running session's mark spins, what needs you pulses, and a session's line flashes for a moment as it finishes or comes to need you, so what is moving stands out, and a title or branch too long for the selected line slides along so you can read it; `MUSTER_MOTION=off` keeps them still.
- **Needs you.** One queue of everything waiting on you: questions first, then news, oldest first. While your terminal window is in the background, Muster tells you as something comes to need you, or an agent finishes. On a Mac, Muster's own helper sends the notification, from any terminal and with every window closed (allow *Muster* under System Settings → Notifications the first time), and clicking it opens that session; elsewhere, a desktop notification where your terminal shows them (iTerm2, WezTerm, Ghostty, kitty) and its bell otherwise. *Notifications* on the Settings screen turns them off, as does `MUSTER_NOTIFY=off`; under it, *Questions*, *Finishes* and *Errors* each turn on or off and, on a Mac, take a sound of their own (← → plays each as you choose). A Mac notification says the project and branch as its title, the chat under it, then what happened, grouped by project.
- **Your agents in the notch.** On a MacBook with a notch, while you look away from Muster and an agent works or waits on you, the notch shows them level with it, in a slim black side either side so the notch looks a little wider and no thicker: how many wait on you on the left, how many work on the right (nothing can be drawn inside the notch itself, which has no pixels). Point at it and it opens downward into the list of them, each with its project, what it is doing and for how long; click one and Muster's window comes to the front with that session open. It keeps showing after you close the UI, as long as agents run. *Agents in the notch* on the Settings screen turns it off (`MUSTER_NOTCH=off` keeps a UI from starting it).
- **Panels of your own.** The list, the session, its changes and a shell beside the agent (`Shift-T`, named by what runs in it: `zsh`, `bun`, `cargo`) sit side by side as you arrange them: drag a panel by its title or the line between two, or move them with `m`. The list takes the width of its names, and `Ctrl-\` folds it away, even while you type in a session. Every key is yours to change, under *Keys* on the Settings screen.
- **A real terminal per session, beside the list.** Work in any session to the right of the list, or full screen, as you would in your own terminal: the daemon answers the program's terminal questions, those about its colours with your terminal's own, so agents choose a light or a dark theme to match it; what a program there copies reaches your clipboard (text only, while you look at it, never again when its history is shown), and nothing there can read your clipboard; a program that follows its terminal's focus is in focus while its session is on screen, so Claude Code's suggested replies show up as you watch; the terminal's size is in pixels too, where your terminal says its own, for programs that size pictures by it; and keys are encoded the way the program asked (kitty keyboard protocol, so Shift+Enter reaches Claude Code; bracketed paste; application cursor keys). Every program there is told its terminal is Muster (`TERM_PROGRAM=muster`), not the one Muster was started from, so none sends that terminal's own sequences or reaches into it.
- **Survives restarts.** Sessions belong to the daemon, which runs on when the UI closes, and on a Mac when you quit the terminal app it ran in too (launchd starts it, unless over SSH). After a daemon restart, agent sessions resume their conversations (`--resume`, `codex resume`, `opencode --session`), one at a time a moment apart; one that cannot falls back to a fresh start. A conversation is resumed in one session only: a second session on it is left ended and says why.
- **Tickets, wherever they are.** A screen of each project's open tickets — GitHub or GitLab issues (through your `gh` or `glab`), Jira work items (Cloud or Data Center), or Linear issues — with the agents working on each and how they are doing. Assign an agent to a ticket and Muster creates `muster/issue-<key>-<title>` (`#42`'s, or `APP-7`'s; without `muster/` if you turn *Branches with muster/* off on the Settings screen), starts the agent you choose there (Claude Code, Codex, Gemini CLI, OpenCode or Copilot CLI), and hands it the ticket — marked as a description of the work, not as instructions, since anyone can file one. A credential someone pasted into a ticket (a token in a `curl` command, an API key, a password) is shown and handed on as `[hidden by Muster]`; the ticket itself is left as it is. More agents on the same ticket get worktrees of their own. Repositories that take their tickets from one Jira project (or Linear team) share them: each ticket is listed once, and assigning an agent asks which repository it is for. When the work is done, close the ticket from Muster (`x`, after asking) and let it close the ticket's agents and remove its worktrees, keeping the branches. That is the only change Muster makes in a tracker.
- **The stages by hand.** On any worktree: run the project's checks as a session, have Claude Code review the change in plan mode, or push the branch and open a draft pull request.
- **What changed, at a glance.** Each worktree's line says what its work comes to (`+120 −30`), looked at again as an agent's turn there ends. `d` shows it beside the list: the files not yet committed, the branch's commits since it left the branch it merges into (`origin/main`, or `base_branch` in `muster.toml`), and the files they changed, each with its lines; `⏎` shows a file's diff. Git is only asked; nothing in the worktree changes, and no code is stored or logged.
- **Undo an agent's turn.** Every agent's turn in a worktree is kept, whatever the agent, with what it changed, and so are the edits between turns. `d` lists them under the changes; `⏎` shows what one changed, and `z` puts the files back as they were before it, once you say yes, and the branch too where its commits are not pushed. Never while an agent works there, and what was there is kept first, so an undo can be undone. The snapshots live in Muster's own store, not your repository.
- **Chats with names and faces.** Each session's line starts with its agent's logo (pictures in iTerm2 and WezTerm, the agent's mark in its colour elsewhere) and is named by the first line of its first prompt, credentials hidden. Once the work is under way, Claude Code or Codex (*Named by* on the Settings screen, with a small model) renames it in a few words, and again after its first answer; a name you gave it is never replaced. `R` renames it.
- **A calm look, the same in any terminal.** Muster draws in its own soft colours, with one accent for what has your keys and a selected line shaded from your terminal's own background. Each session takes two lines, its name whole and what it is doing under it. On the Settings screen, *colours* switches to your terminal's own and *lines for each session* to one line (`[look] colours = "terminal"`, `[look] session_lines = 1` in `settings.toml`).
- **Tidy a branch's commits.** `t` has the agent you pick rewrite the commits not pushed yet into clear ones, never while an agent works there. When it stops, Muster checks that the files are exactly as they were and says so on its line (`tidied 5 commits into 2, files unchanged`); its turn is in the timeline, so `z` puts it all back.
- **Stack work on work.** When you assign an agent to a ticket, `←` `→` make its worktree on top of another worktree's branch, so it builds on work not merged yet. It is measured, reviewed and opened as a pull request against that branch. `u` moves it onto that branch's newest commit, or onto your base once that branch has merged (or any branch onto the newest of its base), after you say yes; nothing changes if a commit would conflict, and `z` puts it back.
- **See where agents' work collides.** When two worktrees of a project change the same files (or an agent's worktree and your checkout do), both lines say so: `overlap` in yellow, or `conflict` in red where git's own merge says merging both would conflict, looked at again as each turn ends. A worktree's details and its `d` pane name the other worktree and the files. Git merges them in Muster's own store; nothing in either worktree changes.
- **Pull requests in view.** Each worktree shows the pull request from its branch, as GitHub, GitLab or Bitbucket last said (about every minute while the UI is open): `#42 approved ✓`, `#17 draft ✗`, `#9 merged`. Its details say it in words, `b` opens it in your browser, and a ticket's agents show theirs. Muster only reads it.
- **What agents used.** Each session's pane says what its agent has used so far, read from the agent's own records: Claude Code's transcripts, Codex's rollouts, Gemini CLI's chat recordings, and OpenCode through its plugin, which also says the cost (`1.2M tokens`, `$0.42 · 40k tokens`; cached tokens count). A ticket's agents say theirs, and all together. Copilot CLI records its usage only as it exits, so its sessions say none.
- **Resources.** Memory and CPU for each session's whole process tree, and the machine's memory.

## Security and privacy

- **Local only.** No Muster server, account or telemetry. Muster talks only to your trackers, your git host and the agents you run, and to a plugin's repository when you install or update it.
- **Your agents, your sign-ins.** Agents run as you, in your own environment, as if started in your terminal; Muster adds no credentials and never edits your agents' settings or your repository.
- **Sign-ins in the Keychain.** Jira and Linear (browser sign-in through their MCP servers' OAuth) and API tokens are kept in the macOS Keychain, never in a file, the database or a log.
- **Tickets are untrusted.** A ticket reaches an agent fenced off as data, never as instructions, and without its control characters. Credentials someone pasted into a ticket are replaced with `[hidden by Muster]` before the ticket is shown or handed to an agent.
- **A private daemon.** The daemon's sockets are open to your user only, and every session's hooks carry a token of their own. The daemon runs apart from the UI, so closing a terminal never takes your sessions down.

Found a security problem? Please open a private security advisory on GitHub rather than a public issue.

## Install and run

Muster runs on macOS (Apple Silicon and Intel) and Linux (x86-64 and ARM). It needs git, and — for the features that use them — the agents you want to run (Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI), and the GitHub CLI (`gh`) or the GitLab CLI (`glab`) for your git host, signed in.

Install or update it with one command:

```bash
curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/main/install.sh | sh
```

It downloads the prebuilt binary for your machine from the latest release, checks it against its published checksum and, if the [GitHub CLI](https://cli.github.com) is signed in, against the attestation GitHub made as the release workflow built it, and puts it in `~/.local/bin`. `MUSTER_VERSION=v0.3.15` installs that release instead. On a machine without a prebuilt binary it builds the same release from source, which needs [Rust](https://rustup.rs). `muster upgrade` checks a new release the same way.

On a Mac it asks whether to install the Muster app too, the window over the same daemon (`PRODUCT.md` §6a), from the same release, checked the same way, into `/Applications` (or `~/Applications`). Answer for it ahead with `sh -s -- --app` or `--no-app` (`MUSTER_APP=1` or `0`), as a script would; an app already there is simply updated. The app is not signed by Apple, only ad hoc: installed this way it carries no quarantine, so macOS opens it with no question. Copied from a browser download instead, macOS refuses it until you allow it once in System Settings › Privacy & Security › Open Anyway.

Later, `muster upgrade` installs the newest release the same way, but only once the attestation is checked (sign in to the GitHub CLI with `gh auth login`, or set `MUSTER_SKIP_ATTESTATION=1` to take the checksum alone), the Muster app too where it is installed, and moves the running daemon onto it in place: every session keeps running, a shell's work and an agent's turn included, and the UI reconnects by itself. When a new version changes how Muster's database is laid out, it first keeps a copy of the database as it was, in `backups/` in Muster's data folder (the newest three), and does not start if it cannot. After installing some other way, `muster reload` does the second half. (A daemon from before 0.2.0 cannot restart in place: stop it once with `muster daemon stop`; agents resume their conversations when it starts again.)

```bash
muster
```

`muster` opens the UI and starts the daemon if none is running. Press `a` to add a project: the dialog lists the folders where you started it, git repositories marked. Type to narrow them, `↑` `↓` to choose one, `⇥` to open it and `⇧⇥` to go up a folder; `⏎` adds the chosen folder, or the folder typed when none is chosen (at first, the one you started in). Any folder can be a project, a git repository or not.

Every key below is Muster's until you set another: *Keys* on the *Settings* screen, or `[keys]` in `~/.config/muster/settings.toml` (`new_session = "N"`, `leave_session = ["Q", "alt-q"]`). The hints, the help and the menus always show your keys.

| Key | In the list |
|---|---|
| `↑` `↓` | move (the selected session streams in beside the list) |
| `⏎` | type in the selected session, beside the list; on a worktree or checkout (`⌂`), start a session; on a project, create a worktree (on a plain folder, start a session in it) |
| `f` | the selected session full screen |
| `e` | fold the selected project or worktree, hiding what is under it (its line still shows its most urgent session's mark, and how many it hides), or unfold it; on a session, fold its worktree. A click on a line's `▾` `▸` does the same |
| `a` | add a project |
| `w` | new worktree for the selected project |
| `n` | new session in the selected worktree, or on a project in its own checkout: a coding agent with an optional first prompt, or a shell (`⇥` `⇧⇥` choose; it offers the one you last started first) |
| `i` | the Tickets screen, on the selected project |
| `s` | stop a session; it stays listed, for `r` |
| `R` | rename the selected session's chat |
| `r` | start an ended session again: an agent resumes its conversation, a shell starts afresh |
| `x` | close the selected session: a running one after asking once (it stops, then leaves the list), an ended one at once; or, after asking, the selected worktree, or a project (Muster forgets it; its files are untouched) |
| `c` | run the project's checks in the selected worktree, as a session (`r` runs them again) |
| `v` | have Claude Code review the selected worktree's change, in plan mode (it changes nothing) |
| `t` | have the agent you pick tidy the selected worktree's commits not pushed yet; Muster checks the files end as they began |
| `u` | move the selected worktree's branch onto the newest of what it is stacked on, or of its base, after asking; nothing changes if a commit would conflict |
| `o` | push the selected worktree's branch and open a draft pull request from it |
| `b` | open the pull request from the selected worktree's branch in your browser |
| `Shift-T` | a shell in the selected session's workspace, beside it, with the keys (the one it has, or a new one); named by what runs in it (`zsh`, `bun`, `cargo`), and listed on one line under its worktree's chat (or after the chats, where several share the folder); `Shift-T` again puts it away, still running. A click moves the keys between the session and its shell |
| `d` | what the selected worktree's work has changed, or the selected session's, in a panel beside the session (beside the list where the window is narrower), with its agents' turns: `↑` `↓` choose a file or a turn, `⏎` its diff, `r` asks git again, `esc` goes back a step, `d` back to the list |
| `z` | (in `d`, on a turn) put the files back as they were before it, after asking; `b` there moves the branch back too |
| `⇥` `⇧⇥` | the next or previous tab: *Sessions*, *Needs you*, *Tickets*, *Settings* |
| `Ctrl-1` `Ctrl-2` `Ctrl-3` | straight to a tab, from anywhere, while typing in a session too (on the list, `1` `2` `3` as well, for terminals without the kitty keyboard protocol, which cannot tell Ctrl and a number from the number) |
| `4` | the *Settings* tab, from the list |
| `Ctrl-\` | fold the list away, so the session takes its place, or bring it back; while typing in a session too |
| `m` | move the panels with the keys: `⇥` picks one, the arrows move it, `+` `-` make it wider or narrower, `⏎` keeps it, `esc` puts it back |
| `esc` `⌫` | back to *Sessions* from the other tabs |
| `l` | the session that needed you last, as notified (a finish is told once it has stayed one two seconds) |
| `h` | every key of the screen shown; type to narrow them (`?` works too) |
| right-click | on a session, worktree or project: what can be done there, each with its key; choose one, or press its key |
| `q` | quit the UI (sessions keep running) |

While you type in a session, beside the list or full screen, every key goes to the program, Esc, Backspace and `⇥` included (Claude Code and shells need them), but two: **Shift-Q** gives the keys back to the list, and so does a click on it; **Ctrl-\** folds the list away and back. So a capital Q cannot be typed in a session; Ctrl-Q reaches the program. A session is sized to where it is shown, so Claude Code draws for the pane it is in.

The panels are yours to arrange: drag one by its title to another edge of the window or of another panel, where the place lights up, and drag the line between two to make one wider (or the column just before it). The title or line under the pointer lights up, to show what you can drag, and in iTerm2 (3.7 or later), kitty and Ghostty the pointer turns into arrows over a line and a hand over a title (`MUSTER_POINTER=off` leaves it alone). The list starts as wide as its names need (30 to 36 columns). Where the panels are is kept in `~/.config/muster/settings.toml` (`[layout] panels`); *Panels* on the *Settings* screen puts them back as at first.

| Key | On the *Tickets* screen |
|---|---|
| `⏎` | open the agent on the selected ticket, or start the first: choose the agent (`⇥` `⇧⇥`, the last one chosen first), and it starts in a worktree for it with the ticket as its first prompt |
| `a` | start another agent on the ticket, chosen the same way, in a worktree of its own |
| `←` `→` | in that choice, where a new worktree is made: on top of which worktree's branch, or the base |
| `↑` `↓` | in that choice, where several projects take their tickets from the same Jira project (or Linear team): which of them the ticket is for, the last one chosen first |
| `b` | open the ticket in your browser |
| `x` | close the ticket in its tracker, after asking; then, if you want, close its agents and remove its worktrees (the branches are kept) |
| `r` | fetch the tickets again |

| Key | On the *Settings* screen |
|---|---|
| `⏎` `space` | turn the selected setting on or off, per project: *Only my tickets*, which lists only the tickets assigned to you, in whichever tracker the project's tickets are; *Branches with muster/*, whether the branches Muster makes start with `muster/`; and, on a Mac, Muster's own *Agents in the notch* |
| `←` `→` | on *Session beside the list from*, a smaller or a larger width (`⏎` goes to the next): in a window narrower than it, the list takes the whole window and `⏎` opens a session over it |
| `⏎` | on *Panels*, every panel back where it was at first |
| `⏎` | on *Notifications*, turn them on or off. On a Mac, Muster's helper sends notifications of the Mac's own, in any terminal and with every window closed, once you allow Muster's notifications (macOS asks once; later, System Settings, Notifications, Muster); a click opens the session |
| `⏎` `←` `→` | on *Questions*, *Finishes* and *Errors*: `⏎` tells that kind or not; `←` `→` choose its sound, the Mac's own, none, or one of the Mac's sounds, played as you choose it. By hand: `[notifications] finishes = false`, `questions_sound = "Glass"` (or the name of a file in `~/Library/Sounds`) |
| `⏎` | on *Keys*, every key by what it does: `⏎` on one and then the key you want sets it (refused where another action has it on the same screen), `⌫` puts it back, `Shift-R` puts every key back |
| `⏎` | on *Chats named by an agent*, turn it on or off; on *Named by*, the next agent that can name chats (each session's own first); on *Its model*, type the model or pick one it offers (`⇥`) |
| `r` | read the settings again, after editing a file by hand: yours for the projects, and Muster's own, with the agents you added (*Your agents* says what is there, or which key does not read) |

A change on the *Settings* screen goes into your own settings for the project (`~/.config/muster/projects/<project id>.toml`, over its `muster.toml`; comments and the rest of the file are kept), never into the repository. Muster's own settings, for all of it, go into `~/.config/muster/settings.toml`.

The mouse works too. The wheel scrolls back through a session's output, or reaches the program if it handles the mouse itself, as Claude Code does. In the list it moves the selection, or scrolls the preview (as do `PgUp`/`PgDn`, with `End` back to the newest output); a click selects a line, and a click on the session beside it gives it the keys. To select text in your terminal while Muster has the mouse, hold Option (iTerm2, Terminal.app) or Shift (most others) as you drag.

Start agents straight from your shell. Each one gets its own worktree, with a branch named from its prompt, and runs in the background, so you can start the next one at once:

```bash
cd ~/code/app
muster claude "Fix the login redirect"
muster codex "Add tests for the session list"
```

Then see them, wait for them and read what they did. `wait` says by its exit status how each one ended: 0 finished, 3 needs you, 124 out of time.

```bash
muster list agents
muster wait a1b2c3 && muster logs --reply a1b2c3
muster prompt a1b2c3 "Now run the tests"
muster attach a1b2c3
```

An agent can start agents the same way, within limits you set on the Settings screen. Each one it starts says so.

Or work inside a workspace (a worktree) from your terminal, as you would with tmux. `muster enter` turns the tab into the workspace's terminal. Whatever you start there belongs to that workspace and shows in the same tab. Ctrl-Q goes back one step, and everything keeps running. To see your other agents, list them in another tab with `muster list agents`:

```bash
muster workspace add fix-login     # a worktree on muster/fix-login
muster list workspaces             # your workspaces, each with its sessions by number
muster enter fix-login             # this tab is now the workspace's terminal
muster list agents                 # (in another tab) this workspace's agents; --all for everything
claude "Fix the login redirect"    # a session of this workspace, shown here; Ctrl-Q comes back
muster term new                    # another terminal in the same workspace
muster attach fix-login/1          # from anywhere, straight back into session 1
```

Drive a terminal from another tab:

```bash
muster send fix-login/1 npm run dev        # type a command and press Enter
muster wait fix-login/1 --for "ready on"   # until it shows that text
muster keys fix-login/1 ctrl+c             # press keys by name: enter, esc, up, f5…
muster move fix-login/1 --to api-cache     # a terminal into another workspace
```

Run agents on a machine of your own (a server, a cloud VM such as a DigitalOcean droplet, a Linux VM on your PC, a Mac at home: macOS or Linux, Intel/AMD or ARM, anything you can `ssh` into), so they keep working while your laptop sleeps. The first `muster ssh` there is where ssh may ask you for a password or to trust the machine; after that, connections are shared and never stop to ask. If your ssh forwards your agent (`ForwardAgent yes` for that host), agents there push with your keys. Muster installs itself there only after asking, says what's missing (Claude Code signed in there, git, gh), and opens Muster on it through your own ssh:

```bash
muster machine add vm you@203.0.113.7   # a name for it; your ssh keys stay ssh's
muster ssh vm                           # check, install after asking, open its Muster
muster ssh vm claude auth login         # a command there, as ssh runs one (sign Claude Code in there)
muster ssh vm --install-claude          # Claude Code there, with its own installer
muster --on vm list agents              # your own muster, asking vm's
muster --on vm attach fix-login/2       # a session there, straight in this tab
muster --on vm                          # your own UI, on vm's agents; reconnects if the link drops
```

`muster ssh vm --install` downloads the machine's release here, checks it, and sends it over ssh, so the machine needs only ssh. To try a branch on a Linux machine before it is released, `scripts/linux-build.sh` builds this checkout for Linux in Docker, and `MUSTER_REMOTE_BINARY=target/linux/muster-x86_64 muster ssh vm --install` puts it there.

Tickets work the same way, in plain words:

```bash
muster list tickets                # the project's open tickets, with the agents on each
muster ticket 42                   # its text and comments
muster ticket 42 claude            # an agent on it, in the workspace you're in
muster ticket 42 assign fix-login  # make a workspace the ticket's
muster ticket 42 open              # in the browser
muster ticket 42 close             # close it, after asking
```

A bare `claude` (or any agent's name) starts a Muster session only after you add `eval "$(muster init zsh)"` to your `~/.zshrc` (or `bash`, `fish`). Muster never edits that file itself. Without the line, type `muster claude`.

The same things work from scripts:

```bash
muster project add ~/code/app
```

```bash
muster workspace add -p app --branch fix-login
```

```bash
muster claude --in fix-login "Fix the login redirect"
```

```bash
muster list tickets -p app
```

```bash
muster ticket 42 claude -p app
```

```bash
muster ticket APP-7 claude -p app
```

```bash
muster ticket 42 close -p app
```

```bash
muster check fix-login
```

```bash
muster review fix-login
```

```bash
muster pr fix-login
```

```bash
muster status
```

```bash
muster list --all
```

```bash
muster restart fix-login/1
```

When a session's status looks wrong, ask why it has it: whom it waits on and for what, whether its turn is held for subagents, its next deadline, and the last signals it received, by kind and tool alone, never what was written.

```bash
muster explain fix-login/1
```

For your own scripts, `muster status` and `muster list` (sessions, workspaces, projects, tickets) take `--json` and print one JSON document instead of words, a session given alike in each. Each starts with `"format": 2`, which changes only when the shape does; a ticket's credentials stay hidden, as on the screen, and no control character reaches your terminal raw.

```bash
muster list --all --json | jq -r '.sessions[] | select(.status == "needs_you") | .title'
```

### Agents of your own

A coding agent Muster does not know can be added in Muster's own settings, `~/.config/muster/settings.toml`, never in a repository, and no Muster release is needed. It is offered after Muster's own when you start a session (`n`, `--harness my-agent`), and, if it takes a first prompt, when you assign one to a ticket or have one tidy commits. Muster never edits the agent's own configuration: it hears it by its terminal's progress signal (OSC 9;4), the words of its window title you name, and its exit, so its status says less than a built-in agent's.

```toml
[agents.my-agent]
label = "My Agent"                    # its name on screen; the key otherwise
program = "my-agent"                  # on PATH, or a path
args = ["--no-banner"]                # always given, first
prompt = ["--message", "{prompt}"]   # how a first prompt is given; leave out for none
resume = ["--continue"]               # given instead after a restart, to carry on
env = { MY_AGENT_THEME = "dark" }     # more environment
mark = "M"                            # shown where Muster's own agents have theirs
colour = "green"                      # one of the terminal's colours, or "#rrggbb"
ask = ["--print", "{prompt}"]         # how it answers one question and ends, to name chats
model = ["--model", "{model}"]        # how a model is chosen for that question
models = ["models"]                   # how it lists its models, one to a line

[agents.my-agent.title]               # words its window title says, if it says any
working = "Thinking"
ready = "Ready"
waiting = "Approve?"                  # shown as needing you
```

Only `program` is needed. With `ask`, it can name chats (*Named by* on the *Settings* screen); give it the arguments that answer one question without tools, your hooks or saved history, as Muster does for Claude Code (`-p --safe-mode`) and Codex (`exec --ephemeral`). A name Muster's own agents use (`claude`, `shell`, …) is refused, and a key that does not read is named on the *Settings* screen's *Your agents* and when you start one; `r` there reads the file again after you edit it. Without `resume`, a session of it ends when the daemon restarts rather than carrying on.

### Where tickets and pull requests are

Muster tells from a project's git remote where it is hosted: GitHub, GitLab (`gitlab.com`, or a host named `gitlab.…`) or Bitbucket. GitHub's and GitLab's projects get their issues as tickets, and pull requests by hand (`muster pr`, `o`) go to the same host — through your `gh` or `glab`, signed in. A project says what its remote cannot in its `muster.toml` (or your local overrides of it, `muster project config`; *Only my tickets* is also on the *Settings* screen):

```toml
[tickets]
tracker = "jira"               # github, gitlab, jira, linear, or a ticket plugin you installed
site = "acme.atlassian.net"    # Jira: the site, or a Data Center's address
project = "APP"                # Jira: the project (or jql = "…")
# team = "ENG"                 # Linear: the team
mine = true                    # only tickets assigned to you, in any tracker

[pull_requests]
host = "gitlab"                # github, gitlab or bitbucket, for a host on another name
```

Jira and Linear are signed in to once, in your browser, the way Claude Code's connectors are (through their MCP servers' OAuth); Bitbucket takes an API token. Muster keeps these in the macOS Keychain, never in a file, the database or a log:

```bash
muster auth jira
```

```bash
muster auth linear
```

```bash
muster auth bitbucket
```

`muster auth jira --token --site <site>` keeps an API token instead (Jira Cloud's, with your email; or a Data Center's personal access token), and `muster auth linear --token` a personal API key. Elsewhere than macOS, or for one daemon, set `JIRA_API_TOKEN` and `JIRA_EMAIL`, `LINEAR_API_KEY` or `BITBUCKET_TOKEN` in the environment it starts in. Bitbucket retired its own issue tracker in 2026, so a Bitbucket project's tickets are named in `[tickets]` (Jira, usually).

After upgrading muster, stop the daemon the old build left running; the next `muster` starts the new one, and Claude Code sessions resume:

```bash
muster daemon stop
```

## Plugins

Muster keeps a lean core and grows through plugins: ticket sources, and panels that show any terminal program beside a session. GitHub, GitLab, Jira and Linear are Muster's own plugins, speaking the same protocol yours would, so a tracker Muster does not know is a small program away, in any language. Muster shows everything a plugin would run before you say yes, and runs none until you do.

```bash
muster plugin install https://github.com/someone/their-plugin
```

Or `a` on the Settings screen's *Plugins*. [PLUGINS.md](PLUGINS.md) lists them; [docs/plugins.md](docs/plugins.md) says how to write one, with [examples](examples/plugins). Yours is welcome.

## How it is built

A Rust workspace of eight crates around one rule: the daemon owns everything, and clients only draw it.

- **`muster-daemon`** — a single Supervisor task owns all state and is the only writer (no shared mutable maps); every change is stored in SQLite before it is broadcast. Each session's terminal is an actor owning a sequence-numbered output ring, so a client that reconnects gets exactly what it missed. Hooks arrive on a private socket and are matched to sessions by a per-session token. GitHub calls run off the Supervisor's loop.
- **`muster-state`** — the status logic as pure state machines with an injected clock: every rule (subagents still working, several open prompts, a permission notice that arrives late) is a unit test.
- **`muster-core`** / **`muster-client`** — the protocol (length-prefixed MessagePack with named fields, typed error codes, version negotiation) and its transport.
- **`muster-model`** — a client's copy of the daemon's state, built from its snapshot and deltas: what needs you and in what order, the list's lines, the spaces. Pure, and shared by the UI and the desktop app so they answer alike.
- **`muster-tui`** — ratatui. The app is a state machine with no IO, so behaviour is tested by feeding it events and drawing it into a test buffer.
- **`muster-notch`** — the notch, on a Mac: a client of the daemon like the UI, drawn through AppKit. What it shows and where are pure functions, tested anywhere.
- **`muster`** — the binary.

[ARCHITECTURE.md](ARCHITECTURE.md) has the details and the rules (`ARC-*`) the code is held to; [PRODUCT.md](PRODUCT.md) has what Muster is for (`PRD-*`).

## Repository blueprint

Every code directory has a `README.md` contract of its own (its interface, invariants and side effects), rolled up here. Test-only folders, fixtures, assets and generated bindings have none.

### Local interface and invariants

- `install.sh` — the one-command installer (`curl … | sh`): downloads the release binary for this machine (`MUSTER_VERSION`, `MUSTER_INSTALL_DIR`), checks its SHA-256 and, where `gh` is signed in, its build attestation (`MUSTER_SKIP_ATTESTATION=1` takes the checksum alone), builds from source where no binary exists, and on a Mac offers the Muster app (`--app`, `--no-app`, `MUSTER_APP`, `MUSTER_APP_DIR`). A download that fails its checks installs nothing (`ARC-21`). `MUSTER_DOWNLOADS` points it at a stand-in for tests.
- `Cargo.toml` — the workspace of the crates below; `apps/desktop/src-tauri` is a Cargo workspace of its own.

### Subdirectories and system boundaries

- crates: [./crates/README.md](./crates/README.md) — the Rust workspace: `muster-core` (IO-free shared types, protocol 58, strict readers), `muster-state` (pure session and run machines), `muster-model` (a client's copy of state), `muster-client` (transport), `muster-daemon` (the Supervisor and everything it reaches), `muster-tui`, `muster-notch` and the `muster` binary.
- apps: [./apps/README.md](./apps/README.md) — the desktop app (Tauri 2 + React 19), a window over the same daemon (`ARC-24`, `ARC-25`, `ARC-26`).
- scripts: [./scripts/README.md](./scripts/README.md) — CI rule checks (`check-*`: file and type size `ARC-08`, secrets `PRD-06`, test sleeps `ARC-30`, TUI colours `ARC-29`, duplication `ARC-31`), agent-session measurement, the Linux build in Docker (`ARC-27`), logo rendering, the daemon soak test, and Claude Code hooks.
- docs, examples: no code contracts; [docs/plugins.md](docs/plugins.md) says how to write a plugin, and each folder under [examples/plugins](examples/plugins) has a README of its own.

### Global invariants

- The daemon owns all state; one Supervisor task is its only writer, and every change is stored before it is broadcast (`ARC-02`, `ARC-10`). Clients only draw it (`ARC-24`).
- `muster-core`, `muster-state` and `muster-model` do no IO (`ARC-01`); Docker, git hosts, ticket sources, harnesses, PTYs and the clock are reached only through traits (`ARC-03`).
- Every protocol change raises its version; frozen handshake samples are added, never edited (`ARC-09`).
- No source file over 1,000 lines, no struct over 40 fields (`ARC-08`); `unsafe` only in the daemon's restart hand-off and the notch's AppKit code (`ARC-12`).
- Tasks are owned and every wait has a deadline (`ARC-13`); queues, rings and logs are bounded (`ARC-14`, `ARC-15`).
- No real credential in code, fixtures, logs or output; credentials pasted into tickets are hidden (`PRD-06`). Ticket and plugin text is data, never instructions, and loses its control characters (`PRD-07`).
- Nothing is written into the person's repository or agents' settings (`PRD-13`, `ARC-05`); plugins run nothing until their manifest's hash is approved (`PRD-15`).

### Global side effects

- Muster's data folder (SQLite store with backups, timelines, plugin folders, run mirrors) and runtime folder (two private Unix sockets, the daemon's lock and the notch's, the daemon log, and the forwarded ssh-agent link).
- Child processes: agents and shells on PTYs, git, `gh`, `glab`, Docker, plugin programs, ssh.
- Network only to the person's trackers and git hosts (Jira, Linear, Bitbucket, MCP and OAuth sign-ins), GitHub releases for upgrades, and a plugin's repository on install or update.
- The macOS Keychain for sign-ins and tokens; on a Mac, notifications and the notch panel.

### System verification

- `cargo fmt --check` and `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets`.
- Tests of what changed: `cargo nextest run -p <crate>` plus the end-to-end files in `crates/muster/tests/` for the area touched (see [CLAUDE.md](CLAUDE.md), *Which tests to run*); `pnpm lint`, `pnpm typecheck`, `pnpm test` in `apps/desktop`; `cargo test` in `apps/desktop/src-tauri`.
- The whole suite (`cargo nextest run --workspace`) only for a protocol, dependency or toolchain change, or a release.

## Development

```bash
cargo nextest run --workspace   # or: cargo test --workspace
```

The toolchain is pinned in `rust-toolchain.toml`; rustup installs it on first use. CI also runs `typos` and `cargo shear` (`cargo install --locked cargo-nextest cargo-shear typos-cli`).

`scripts/soak.sh [minutes]` runs a daemon of its own under a steady churn of sessions and clients and prints its memory and what it holds every 30 seconds: a count that only climbs is a leak.

The suite (over 700 tests) includes end-to-end tests that run the real binary in throwaway directories: real shells on real pseudo-terminals in real git worktrees, a stand-in `claude` and `gh` on `PATH` (so hooks, resume and issue handling are exercised exactly as in use, without an account or network), and the UI itself driven by keystrokes on a pseudo-terminal. CI runs formatting, spelling, clippy and the docs with warnings as errors, a check for unused dependencies, a 1,000-line file limit, `cargo deny`, a build on the oldest Rust it supports, and the tests (through `cargo nextest`) on macOS and Linux.

## License

[MIT](LICENSE).
