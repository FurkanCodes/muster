# githost

## Overview

The submodules of `src/githost.rs`: pull requests on each project's git host
(`ARCHITECTURE.md` §10). GitHub is reached through the person's `gh`, GitLab
through their `glab`, each with the person's own login and the repository found
from the checkout's remote; Bitbucket (Cloud API 2.0 and Data Center) through
its REST API with a token the person gave Muster. All of it sits behind the
`GitHost` trait, with a fake in tests (`ARC-03`).

Muster pushes and opens pull requests as the person, so the person is the
author: the reviewer agent's verdict is posted as a review comment, and a
ready pull request waits in Needs you. The cockpit only reads where each
worktree branch's pull request stands (`pull_request_of`, polled by
`supervisor::pulls`).

## Interface

From the module root `src/githost.rs`:

- `GitHost` (githost.rs) — the trait: `open_draft`, `mark_ready`,
  `mark_draft`, `review_comment`, `state`, `pull_request_of`.
- `PrState` (githost.rs) — where a pull request stands.

In this directory:

- `Hosts` (choose.rs) — `for_project(place)`: finds each project's host (real
  ones in use, a fake in tests).
- `ServiceHosts` (choose.rs) — the real `Hosts`; holds the `Secrets` store and
  an HTTP client for API hosts.
- `choose(written, origin)` (choose.rs) — the host named in
  `[pull_requests] host`, else the remote's host.
- `GitHub` (github.rs) — pull requests via `gh`.
- `GitLab` (gitlab.rs) — merge requests via `glab`, opened as drafts from a
  branch Muster already pushed; the number is the `iid`.
- `Bitbucket`, `Bitbucket::for_remote`, `TOKEN_VARIABLE`, `account`
  (bitbucket.rs) — Bitbucket via REST with a bearer token
  (`muster auth bitbucket` or `BITBUCKET_TOKEN`).

## Invariants

- Hosts are reached only through `GitHost` / `Hosts` (`ARC-03`).
- New pull requests are opened as drafts; `open_draft` returns the branch's
  open one if it already exists, so asking again is safe.
- `glab mr create` is never asked to push; Muster pushes with git first.
- The person's own CLI logins are used for GitHub and GitLab; the Bitbucket
  token is kept in the secrets store or environment and never logged
  (`PRD-06`).

## Side Effects

- Spawns `gh` and `glab` processes.
- HTTPS calls to Bitbucket's REST API.
- Reads the Bitbucket token from the secrets store (macOS Keychain) or
  `BITBUCKET_TOKEN`.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon githost`
- End to end: `cargo nextest run -p muster --test hosts --test runs`
