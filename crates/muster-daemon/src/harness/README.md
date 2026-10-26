# harness

## Overview

The submodules of `src/harness.rs`: what to run for a session
(`ARCHITECTURE.md` §7). The harnesses are `shell` (the person's own shell in
the workspace); the coding agents `claude` (Claude Code), `codex`, `gemini`
(Gemini CLI), `opencode` and `copilot` (GitHub Copilot CLI); `checks` and
`review`, which Muster runs by hand; any coding agent the person added in
Muster's settings (`[agents.<name>]`); and plugin panels
(`plugin:<plugin>/<panel>`). A harness says how to start, resume and hear from
its program; the daemon does the rest.

Each agent is heard the way it allows without editing the person's config
(`PRD-13`): hooks passed at launch, a plugin or settings file of Muster's own
that it is pointed at for the session only, or its window title and terminal
progress sequence. `fixtures/` holds recorded test transcripts and is not
documented here.

## Interface

From the module root `src/harness.rs`:

- `Harness`, `Kind` (harness.rs) — a harness and what it can do:
  `has_hooks`, `can_resume`, `chooses_session_id`, `translate`, `prompt`,
  `spend`, `read_title`, `has_conversation`, `prepare_support`.
- `find`, `find_with`, `coding_agents`, `carried` (harness.rs) — look up
  built-in and person-added harnesses.
- `Launch`, `Place`, `spawn_spec(harness, launch)` (harness.rs) — build the
  `pty::SpawnSpec` (program, arguments, environment) for a session; `Place`
  says whether it runs on the host or in a run container.
- `CheckCommands`, `TitleState` (harness.rs) — the project's checks; what
  `read_title` makes of a window title.
- `hook_command`, `hook_curl`, `HookEndpoint`, `new_token` (harness.rs) — how
  hooks reach the daemon's hook socket with a per-session token.
- `IN_CONTAINER`, `STOP_IN_CONTAINER` (harness.rs) — shell snippets for agents
  in run containers.

In this directory:

- `arguments`, `settings`, `HookPayload`, `translate` (claude.rs) — Claude
  Code's command line and inline `--settings` hooks.
- `AgentEvent`, `TurnEnd`, `TurnOutcome`, `parse_events`, `parse_line`,
  `Lines` (claude_stream.rs) — headless `claude -p --output-format
  stream-json` output; `parse_events` is what run work uses.
- `arguments`, `translate`, `read_title` (codex.rs) — Codex via `notify` and
  its window title; `--no-daemon`.
- `arguments`, `defaults`, `DEFAULTS_VARIABLE` (gemini.rs) — Gemini CLI via a
  system-defaults settings file.
- `arguments`, `config_content`, `PLUGIN`, `spend` (opencode.rs) — OpenCode via
  a plugin merged through `OPENCODE_CONFIG_CONTENT`; reports tokens and cost.
- `arguments`, `manifest`, `hooks` (copilot.rs) — Copilot CLI via a
  `--plugin-dir` plugin.
- `PanelLaunch` (panel.rs) — a plugin panel's program and variables
  (`PRD-15`, `ARC-23`).
- `ActivityChange`, `change`, `tool_named` (activity.rs) — what an agent is
  doing now (`editing view.rs`, `running cargo`).
- `scrubbed`, `kept_from_plugin` (scrub.rs) — daemon variables a session must
  not inherit.
- `prepare`, `opencode_plugin`, `gemini_defaults`, `copilot_plugin`
  (support.rs) — Muster's own support files in its private runtime directory.
- `projects_dir`, `exists` (transcript.rs) — whether Claude Code still has a
  transcript to `--resume`.

## Invariants

- The person's config and repository are never edited: hooks are passed on the
  command line (`--settings`) or through files of Muster's own named for the
  session only (`PRD-13`, `ARC-05`).
- Each hook posts to the daemon's hook socket with the session's own token
  (`ARC-11`); support files hold no secret, the token travels in the
  environment.
- Activity keeps a file's name, never its path, and a command's program, never
  its arguments; nothing of it is stored or logged (`PRD-06`).
- Variables that mark another agent's session or address the launching
  terminal are scrubbed; the person's own settings are kept (`PRD-06`).
- A Claude Code session with no transcript starts fresh instead of
  `--resume`.
- Headless stream parsing reads fields one by one and skips unknown lines, so a
  changed field loses only itself.
- A plugin panel is no agent: no hooks, no resume, never news; it ends with its
  workspace.

## Side Effects

- Writes support files (OpenCode plugin, Gemini defaults, Copilot plugin)
  under Muster's private runtime directory.
- Reads Claude Code's projects directory to check for transcripts.
- Generates random session ids and hook tokens.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon harness`
- End to end: `cargo nextest run -p muster --test harnesses --test claude --test claude_waits --test added_agents --test resumes`
