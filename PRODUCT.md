# Muster — product spec

> Drive your agents yourself, or hand them a ticket and get back a reviewed pull request.

Muster is a local daemon and terminal UI for coding agents. Used hands-on, it is a cockpit like Nebula: start Claude Code, Codex or other agents across projects and worktrees, watch their status, jump into any terminal, and pick up where you left off after closing the UI. Used hands-off, it is a pipeline: it takes a ticket from GitHub Issues (later Jira), runs an agent in its own container, checks the result, opens a pull request, has a second agent review it, and only then asks a person to look. Both run side by side, and work moves between them.

This file is the source of truth for what Muster is and is not. Rule IDs (`PRD-*`) are cited in plans, reviews and tests. When an implementation conflicts with a rule here, change the rule deliberately or change the code — never let them drift.

## 1. Problem

Developers who use Claude Code or Codex every day already run several agents in parallel. Tools like Nebula, Claude Squad and Conductor make that easier: a grid of terminals, one git worktree per agent. They are good **cockpits** for hands-on work. But they stop there. Every agent still needs a person to pick the ticket, write the prompt, watch it, run the tests, open the PR, review it, and send it back when it is wrong — even for routine tickets that do not need a person until the end.

Worktrees also isolate only files. Agents still share ports, databases, caches and processes, and nothing stops two Gradle builds from exhausting memory together.

## 2. Product

Muster is a **cockpit for agents working on your tickets**.

- **Tickets.** Muster fetches each project's tickets (GitHub Issues; Jira Cloud and Linear next) and shows them with the agents working on each. The person assigns agents to tickets: each agent is a session of the coding agent they choose (Claude Code, Codex, …) in a worktree of its own, started with the ticket as its first prompt. Muster reads tickets, and makes one change in a tracker: it closes a ticket when the person asks it to (after asking them to confirm, since everyone watching the ticket is told), and then offers to close the ticket's agents and remove its worktrees, keeping the branches. Any other change (comments, labels, reopening) is the person's, or their agents' through the tools those already have (such as a tracker's MCP server).
- **Cockpit (hands-on).** Every agent is a *session* you can watch and type into. Sessions live in a *workspace*: a git worktree on your machine, the project's own checkout, or a container. A project is usually a git repository, but can be any folder: a plain folder has sessions and checks in the folder itself, and none of what needs git (worktrees, tickets, reviews, pull requests) until it becomes a repository. Muster shows which sessions are running, finished, or waiting for you, and keeps them alive when the UI closes.
- **Autopilot (set aside 2026-10-11).** A *run* took a ticket through fixed stages on its own, in a container, and asked a person only once the work had passed checks and a review. It was built and tested (M2) and then set aside at the author's call: hidden from the UI and the command line's help, its code kept. The rules and stages below that concern runs describe that code.

```
Ticket → Build → Check → Draft PR → Agent review → Ready for you → Merged
           ▲       │                      │              │
           └───────┴──── failure + reason ┴──────────────┘
```

- **The stages are also tools.** On any hands-on session you can run the project's checks, open a draft PR, or ask for an agent review by hand — or hand the session to the autopilot to finish from *Check* onward. And any run's agent can be taken over by hand and given back.

## 3. Users

- **Primary:** a developer or small-team lead who already uses coding agents daily. Some days they want to steer every agent; other days they have a backlog of well-scoped tickets in a repo with tests.
- **User #1:** the author, on Lamphaus (Android, Kotlin, Gradle) and on Muster itself. Lamphaus is a deliberately heavy test: large builds, strict project rules in `RULES.md`, real release discipline.
- **Not for v1:** large teams, non-developers. Without a test suite the autopilot has little to check; the cockpit still works.

## 4. Principles

- **PRD-01 — The person assigns the work.** Muster is a terminal UI for sessions the user drives, on the tickets they choose. Nothing starts on its own: an agent works on a ticket because the person assigned it.
- **PRD-02 — Nothing unverified reaches a person.** A run reaches *Ready for you* only after the project's checks pass and the reviewer agent approves. Failures go back to the building agent with the evidence, up to a per-project attempt limit; after that the run goes to *Needs you* with the full history.
- **PRD-03 — Bring your own agents.** Muster wraps existing harnesses (Claude Code, Codex, OpenCode, …) through one interface. It ships no model and no agent of its own, and uses the user's own subscriptions or keys.
- **PRD-04 — The right isolation for the job.** Hands-on sessions default to a git worktree on the host: instant, with the user's own tools and sign-ins. In a project that is a plain folder, not a git repository, there is nothing to isolate with, so they run in the folder itself. Any session can run in a container instead. Runs always use containers, and a container can never write to the host filesystem.
- **PRD-05 — Resources decide, not just display.** Muster shows memory and CPU for every session and container. Runs carry a memory reservation and start only when it fits the budget; the UI always says why a run is waiting. Hands-on sessions never wait — the user asked for them.
- **PRD-06 — Secrets never leak.** Credentials Muster handles are injected at run time with the narrowest scope available, and never enter the repository, an image layer, a log, a PR, or the database. A credential someone pasted into a ticket is hidden before Muster shows the ticket or hands it to an agent; Muster is never what passes it on. A worktree session runs as the user with the user's own sign-ins, exactly as if started in a terminal; Muster adds none.
- **PRD-07 — Tickets are untrusted input.** Anyone can file a GitHub issue. Agents work only on tickets the user assigns them (and runs, set aside, picked up only tickets the user selected). Ticket text reaches agents as data, set apart and named as a description of the work, never as configuration or instructions, and never reaches a terminal with its control characters.
- **PRD-08 — Move freely between hands-on and hands-off.** Any run's agent can be taken over interactively and handed back. Any hands-on session can be handed to the autopilot from *Check* onward.
- **PRD-09 — Survives restarts.** Closing the UI never stops a session or a run, and neither does upgrading Muster: the daemon moves onto a new version in place, every session running on. Container sessions and runs also survive a daemon restart; worktree sessions come back through the harness's own resume.
- **PRD-10 — Local first.** No Muster server, no account, no telemetry. Muster talks only to the user's ticket sources, git host, container runtime and agent providers.
- **PRD-11 — Measured claims.** Every claim about quality, speed or cost in the README is backed by a reproducible benchmark run committed to the repo.
- **PRD-12 — Bounded cost.** Every run has a time limit and an attempt limit, and a cost limit where the harness reports cost. Hitting a limit stops the run and moves it to *Needs you*; it never silently keeps spending. Hands-on sessions show cost where reported but are never stopped by Muster.
- **PRD-13 — Leave the user's setup alone.** Muster never edits the user's global agent configuration or adds files to their repository to make itself work. It passes what it needs at launch: on the command line or in the environment (for example Claude Code's `--settings`), or as a file of Muster's own, in its private runtime directory, that the harness is pointed at for that session only (for example an OpenCode plugin). Where a harness offers no such way, Muster runs it with less status detail rather than editing its config.

## 5. Run stages (autopilot, set aside)

| Stage | Enters when | Does | Leaves to |
|---|---|---|---|
| **Queued** | A ticket is picked up | Snapshots title, body, comments and labels; waits for a memory slot (`PRD-05`) | Build |
| **Build** | A slot is free | Starts the container, checks out a fresh branch `muster/<ticket>-<slug>`, runs the building agent headless with the ticket and project instructions | Check, or Needs you on a limit |
| **Check** | The agent says it is done | Runs the project's configured commands (format, lint, test) inside the container | Draft PR on success; Build with the failing output otherwise |
| **Draft PR** | Checks pass | Pushes the branch and opens a **draft** PR: summary, ticket link, check results | Agent review |
| **Agent review** | Draft PR exists | A reviewer agent (ideally a different harness, `PRD-03`) reads the ticket and diff and posts a PR review: approve, or request changes with line comments | Ready for you on approve; Build with the comments otherwise |
| **Ready for you** | Reviewer approves | Marks the PR ready and requests the user's review | Merged; or Build when the person requests changes |
| **Needs you** | Any limit is hit, or something needs a decision | Waits with the evidence: history, logs, diff, why it stopped | Build (retry with notes), Take over, or Cancelled |
| **Merged / Cancelled** | — | Stops and removes the container; keeps the record | — |

Every transition is recorded with its reason, so the timeline of a run is always explainable.

**By hand.** *Check*, *Draft PR* and *Agent review* are also actions on any hands-on session. They run where the session lives — in its worktree or its container — and report back to the session instead of moving it along. *Hand to autopilot* starts a run at *Check* with that session's work.

## 6. Interface

Terminal UI first (ratatui). The daemon exposes the same API to any client, so a web UI can be added later without changing the daemon.

- **Sessions** (home) — projects, then their worktrees, then session cards with live status: running, finished, waiting for you, what each agent is doing as its hooks tell (`editing view.rs`, `wants to run npm`), and for how long. A count of agents working, waiting and done stays in view on every screen, and each project keeps a colour of its own. The selected session's live terminal is beside the list, one key (or a click) from typing in it, and one from full screen. Dragging over a session's text copies it, and only it, not the list beside it. Start a session from a prompt, in a new worktree or the project's checkout (in a plain folder, the folder).
- **Tickets** — each project's open tickets, with the agents on each and how they are doing. The selected ticket shows its text and its agents. Assign an agent to a ticket (a worktree for it and the coding agent the person chooses, the last one chosen offered first, with the ticket as its first prompt), open the agent on it, or add another in a worktree of its own.
- **Needs you** — one queue for everything waiting on a person, oldest first: sessions asking a question, or finished and not yet seen. This is where the user starts their day.
- **Settings** — each project's settings that the person changes as they go: *only my tickets*, whether the Tickets screen lists only the tickets assigned to the person, in whichever tracker the project's tickets are; and *tickets from*, the tracker its git remote says or one the person names (Jira with its site and a project's key, Linear with a team's key), which is how a project whose remote is on a company's own server, by an address that names no tracker, gets its tickets. A change goes into the person's own settings for the project, never into its repository (`PRD-13`).
- **Resources** — host memory and CPU, and each session's usage.

## 7. Integrations

| Kind | v1 | Later |
|---|---|---|
| Ticket sources | GitHub and GitLab issues (the person's `gh`, `glab`); Jira (Cloud through Atlassian's MCP server with the person's browser sign-in, or Cloud and Data Center with a token); Linear (its MCP server, or an API key) | Others behind the same interface |
| Git host | GitHub, GitLab, Bitbucket (Cloud and Data Center, with a token) | Others behind the same interface |
| Harnesses | Claude Code, Codex, Gemini CLI, OpenCode, Copilot CLI (hands-on) | Others through config; headless runs beyond Claude Code |
| Container runtime | Docker API (Docker Desktop, OrbStack, Colima on macOS; Docker Engine on Linux) | Podman |

Each kind sits behind one interface from day one, so adding Jira or Codex never touches sessions or tickets.

## 8. Milestones

Each milestone ends with a demo that works on a real repository, and with tests that prove it.

1. **M1 — Cockpit.** The daemon owns hands-on Claude Code sessions in git worktrees. The TUI shows projects, worktrees and session cards with live status, the live terminal, and *Needs you*. Sessions start from a prompt or a GitHub issue. Closing the TUI stops nothing; restarting the daemon brings sessions back through Claude Code's resume.
   *Done when:* the author builds Muster with Muster, instead of plain terminals, for a full week. A container spike (headless Claude Code in Docker) runs alongside, so M2 starts on proven ground.
   *Status (2026-10-11):* built and dogfooded in a pseudo-terminal against Claude Code 2.1.296, with over 350 tests. Beyond the first build: sessions in the project's own checkout; projects that are plain folders, not git repositories; restarting (Claude resumes) and removing ended sessions; worktrees and branches kept in step with git; mouse scrollback; notifications while the window is out of focus; a session asking before it starts (Claude's trust question) shown as needing you; untrusted issue text kept from terminals; `muster daemon stop` for upgrades. Waiting on the author's week of dogfooding.
2. **M2 — One ticket, end to end (set aside).** Containers as workspaces; one GitHub issue goes through every run stage; *Check*, *Draft PR* and *Agent review* by hand on any session; take over and hand back.
   *Done when:* selecting an issue produces a draft PR, a reviewer-agent review on it, and — after a requested change has been fixed — a PR marked ready and waiting in Needs you (GitHub lets nobody be asked to review their own pull request, and Muster opens it as the user). Killing and restarting the daemon mid-build resumes the run.
   *Status (2026-10-11):* the run machine, runs in the store, orchestration in the daemon and the real work (mirror, container, headless agents and checks as jobs, bundle out and push, draft pull request, reviewer verdict) are built and tested, the last against real Docker and git with a stand-in agent; runs are started, followed, retried, sent back with changes, taken over, handed back and cancelled from the command line (`muster run`), and the same from the UI: a Runs view (waiting on the person first, each run's timeline with reasons), runs in Needs you with notifications, and the issues dialog handing an issue to the autopilot; a run's container appears in the UI with the person's sessions in it. A worktree's committed work can be handed to the autopilot for an issue (`muster run start --from`, `p` in the UI): the run starts at the checks. End-to-end tests on real Docker, with stand-ins for Claude Code and `gh`, take an issue to a pull request marked ready through a daemon killed mid-build, and a person through taking a run over in its container and handing it back. The stages run by hand on any worktree: the checks (`muster check`, `c` in the UI), a draft pull request (`muster pr`, `o`) and an agent review in plan mode (`muster review`, `v`). Next: a first real run, which needs a Claude Code token (`claude setup-token`, `muster auth claude`) and a repository to open pull requests on.
   *Set aside (2026-10-11):* at the author's call, the autopilot is hidden: Muster is to assign agents to tickets the person chooses, not to work tickets alone. Its code, tests and the stages by hand (*Check*, *Draft PR*, *Agent review*), which stay in use, are kept.
3. **M3 — Tickets.** A Tickets screen: each project's tickets with the agents on them; assign an agent (a worktree and the coding agent chosen, with the ticket), open it, add another, and close the ticket once the work is done. Read-only towards the tracker, but for closing a ticket the person asks to close.
   *Done when:* the author starts their day on the Tickets screen and assigns agents from it, for a week.
   *Status (2026-10-11):* built for GitHub Issues through `gh`, in the UI (`i`, `⇥`) and the command line (`muster issue list`, `muster issue start --another`). Next: Jira Cloud and Linear.
4. **M4 — More trackers.** Jira Cloud and Linear as ticket sources, read-only but for closing a ticket.
   *Status (2026-10-12):* tickets come from each project's own tracker, named in `[tickets]` or told by its git remote: GitHub and GitLab issues through the person's `gh` and `glab`; Jira (Cloud through Atlassian's MCP server with the person's browser sign-in, as agents' connectors do, or Cloud and Data Center with a token); Linear (its MCP server, or an API key). Tickets are keyed `#42` or `APP-7`. Pull requests by hand go to the project's host: GitHub, GitLab (merge requests through `glab`) or Bitbucket (Cloud and Data Center, with a token). Sign-ins and tokens live in the Keychain (`muster auth jira|linear|bitbucket`). Built and tested against stand-ins of every service and checked against each one's documentation and live metadata; a first real sign-in is the person's.
5. **M5 — Many at once (with the autopilot; set aside).** Memory-aware scheduler, queue reasons, the Resources view, run limits (`PRD-12`).
   *Done when:* five runs on a machine that fits three run three, queue two with a visible reason, and finish all five.
6. **M6 — More harnesses.** Codex and OpenCode, hands-on.
   *Status (2026-10-12):* Codex, Gemini CLI, OpenCode and GitHub Copilot CLI start, report their state and resume, hands-on, beside Claude Code (`ARCHITECTURE.md` §7), each heard the way it allows without editing the person's config: Codex through its turn notice and its window title (its hooks need trust Muster does not give them), Gemini CLI through hooks in a defaults file of Muster's own, OpenCode through a plugin of Muster's own, Copilot CLI through a plugin folder of Muster's own. Checked against Codex 0.162.1, Gemini CLI 0.63.0, OpenCode 1.4.6 and Copilot CLI 1.0.95. Any of them can be assigned to a ticket (2026-10-13).
7. **M7 — Benchmark and release.** (The benchmark below measured the autopilot; it is to be rethought for assigned agents.) A fixed task set (real closed issues with known fixes), run single-agent vs Muster; publish solve rate, time to ready and cost per merged PR. Write-up, demo video, `v0.1.0`.

## 9. Not in v1

- An agent or model of Muster's own (`PRD-03`).
- A web UI (the API keeps it possible).
- Windows.
- Teams, accounts, or any hosted service (`PRD-10`).
- Merging to the default branch without a person's approval.
- Cloud or Kubernetes sandboxes.

## 10. Success measures

- **Cockpit:** the author's daily driver; time from "needs you" to the user seeing it; no session lost to a closed UI or a daemon restart.
- **Autopilot:** share of runs that reach *Ready for you* without a person; share of ready PRs merged without further changes; median time from pickup to ready; cost per merged PR.
- **Engineering:** end-to-end tests that drive a real daemon, real PTYs, real containers and a fake harness; restart survival tested; no secret ever found in logs, images or the database (tested).

## 11. Open questions

- **Agent sign-in inside containers.** Checked 2026-10-11:
  - *Claude Code* documents `claude setup-token`, which prints a one-year OAuth token for CI and scripts, passed as `CLAUDE_CODE_OAUTH_TOKEN`. It needs a Pro, Max, Team or Enterprise plan. `ANTHROPIC_API_KEY` and `apiKeyHelper` (for rotated or vault-fetched keys) also work. `--bare` mode ignores the OAuth token.
  - *Codex* documents `CODEX_API_KEY` for `codex exec`, with `--json` for JSONL events. Saved ChatGPT sign-in is described as an advanced option for trusted runners.
  - *Risk:* anything the agent runs — including the project's own tests — can read a credential held in the container's environment. OpenAI's own guidance warns against exposing the key to repository-controlled code. For `PRD-06`, the likely answer is a host-side credential proxy: the container gets only a per-run token for a local proxy (`ANTHROPIC_BASE_URL` / equivalent), and the proxy adds the real credential. M2 may start with per-exec environment variables on the user's own repositories, with the proxy required before running tickets from anyone else.
- **Headless runs.** The architecture assumes run agents are headless with structured output, and interactive only on take-over (`ARCHITECTURE.md` §7). The container spike during M1 confirms it.
- **Status for hands-on sessions without editing config.** Claude Code accepts hooks through `--settings` at launch (`PRD-13`). Checked one by one on 2026-10-12: Gemini CLI reads hooks from a defaults file named in its environment, OpenCode loads a plugin named in its environment, Copilot CLI loads a plugin folder named on its command line; Codex runs hooks passed at launch only once the person has trusted them, so Muster hears it through its turn notice and its window title instead. Where none of this works, status falls back to the terminal's progress signal and process state.
- **Claude's trust prompt in every worktree.** Checked 2026-10-11 with Claude Code 2.1.296: accepting the prompt in a worktree records the trust against the repository's main checkout, so a repository's other worktrees, and later ones, are not asked again. The prompt therefore appears once per repository not yet trusted. Muster must not answer it or edit Claude's own files to avoid it (`PRD-13`). Claude Code runs no hook while it shows, so a session that has not announced its start within 10 s (it does in under a second) reads as *needs you: asking before it starts*, which also covers a sign-in or first-run setup.
- **Background work at a turn's end.** Claude Code 2.1.291's `Stop` payload carries `background_tasks` (empty in the one capture so far). If it lists background shells and subagents still working, it could replace part of the subagent-drain timing in the status machine. Needs a capture with background work running.
- **Heavy builds.** Android/Gradle in Linux containers needs a large image and several GB per build. Shared dependency caches between containers may be required for Lamphaus to be practical.
