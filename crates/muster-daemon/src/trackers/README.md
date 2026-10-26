# trackers

## Overview

The submodules of `src/trackers.rs`: where tickets come from
(`ARCHITECTURE.md` §10, `ARC-03`). Each project's tracker is chosen by its
settings (`[tickets]` in `muster.toml`, or the person's overrides) or, unsaid,
by its git remote's host, and is asked through its ticket plugin. Muster's own
plugins serve GitHub and GitLab issues through the person's `gh` and `glab`,
and Jira and Linear through their MCP servers (the person's browser sign-in)
or their APIs (a token the person gave). Those built-in trackers run out of
process as `muster plugin-run <id>` (`crate::plugin_run`).

Ticket text is untrusted (`PRD-07`): it is cleaned of control characters as it
arrives and reaches an agent only inside a fenced prompt built here.

`jira/` and `linear/` hold only the tests of `jira.rs` and `linear.rs`.

## Interface

From the module root `src/trackers.rs`:

- `TicketSource` (trackers.rs) — the trait every ticket source implements
  (`ARC-03`): `describe`, `list`, `view`, `close`, `only_mine`, `source`.
- `Trackers` (trackers.rs) — `for_project(place)`: finds each project's
  source (real ones in use, a fake in tests).
- `Hidden` (trackers.rs) — wraps a source so credentials pasted into tickets
  are hidden before anything sees them (`PRD-06`). (`Fixed` is a test-only
  `Trackers`.)
- `IssueDetail`, `Noun` (trackers.rs) — a ticket in full; what a tracker calls
  its tickets.
- `issue_prompt`, `new_fence` (trackers.rs) — the agent's prompt with the
  ticket fenced by a random tag.

In this directory:

- `ServiceTrackers`, `choose` (choose.rs) — pick each project's tracker by
  settings or remote host, among the plugins the registry has.
- `GitHubIssues` (github.rs) — GitHub issues via `gh`.
- `GitLabIssues` (gitlab.rs) — GitLab issues via `glab`; key is the `iid`.
- `Jira`, `Site`, `MCP_URL`, `TOKEN_VARIABLE` (jira.rs) — Jira via Atlassian's
  MCP server or REST (Cloud v3, Data Center v2); closes through the Done
  category transition. For `muster auth`: `MCP_ACCOUNT`, `KeptToken`,
  `token_account`, `EMAIL_VARIABLE`.
- `Linear`, `MCP_URL`, `KEY_VARIABLE` (linear.rs) — Linear via its MCP server
  or GraphQL API; closes to the team's first completed state. For
  `muster auth`: `MCP_ACCOUNT`, `KEY_ACCOUNT`.
- `Plugins`, `PluginSpec`, `TicketPlugin`, `PluginTickets`, `Environment`,
  `is_basic`, `basics`, `refusal` (plugin.rs) — ticket plugins as
  `TicketSource`s, one process per plugin; `basics` is the environment every
  plugin process (and hook run) gets.
  Processes and streams: [plugin/README.md](./plugin/README.md).

## Invariants

- Ticket text is data, not instructions: cleaned of control characters and
  fenced in prompts by a random tag it cannot close (`PRD-07`).
- A plugin's answers are checked and cleaned before anything sees them.
- Closing a ticket, when the person asks, is the only change Muster makes in a
  tracker.
- Muster's own plugins get the environment sessions get; anyone else's get only
  the variables their manifests name.
- Tokens are kept in the Keychain or read from the environment and never
  logged (`PRD-06`).
- Trackers are reached only through `TicketSource` / `Trackers` (`ARC-03`).

## Side Effects

- Spawns `gh`, `glab` and ticket-plugin processes.
- HTTPS calls to Jira, Linear and their MCP servers.
- Reads tokens from the macOS Keychain and the daemon's environment.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon trackers`
- End to end: `cargo nextest run -p muster --test trackers --test ticket_plugins --test issues --test cli_tickets`
