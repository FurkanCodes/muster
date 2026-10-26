# tickets

## Overview

The Tickets screen: each project's open tickets and the agents working on
them. `data.ts` makes one vanilla Zustand store per bridge. The store holds
issue lists, ticket texts, the *brief* (which parts of a ticket an agent is
given), the part the keys are on, the Mine/All toggle's busy state, and the last
agent and project used. Projects that share a ticket source are one group.
Lists are fetched lazily while the screen is visible. The right pane
(`Detail`) shows the ticket's text and comments. Two dialogs load lazily:
`Assign` starts an agent, `CloseTicket` closes the ticket and cleans up.

## Interface

- `Tickets` (Tickets.tsx, default export) — the screen: grouped project lists, selection, hints, keys and dialogs.
- `ticketsFor`, `TicketsModel` (data.ts) — the per-bridge ticket store: `fetch`, `view`, `prune`, `forgetFailed`, `choose`, `reach`, `refresh`, `follow`, `mine`, `closed`.
- `TicketData`, `Load<T>`, `Issues`, `Viewed`, `ticketId` (data.ts) — store shape, load states, `project:key` ids.
- `TEXT_ALONE`, `gives`, `toggled`, `givenWords` (data.ts) — the default brief and its toggle (mirrors the TUI's `TicketBrief::toggle`) and wording.
- `ticketSources`, `groupProjects` (data.ts) — a fingerprint of the ticket plugins; projects sharing a source merged into one group.
- `Assign` (Assign.tsx) — choose agent, project and base branch, then `StartFromIssue`.
- `Detail` (Detail.tsx) — facts, the agents on the ticket with their spend, text and comments, the assign bar.
- `worktreesFor`, `agentsOn`, `agentToOpen` (agents.ts) — a ticket's worktrees, the sessions on them, the one ⏎ opens.
- `issueOfBranch`, `ticketBranch`, `shownKey`, `ticketPullRequest` (agents.ts) — a TypeScript port of `muster_core::tickets` branch naming.
- `CloseTicket` (Cleanup.tsx); `Conversation`, `TextPart`, `partId`, `Choosing` (Conversation.tsx); `TicketText`, `ticketText` (TicketText.tsx); `TicketRow`, `ProjectName`, `ProjectSwatch` (TicketRow.tsx); `oneTicketOf`, `trackerName`, `repositoryOf` (words.ts).

## Invariants

- `PRD-15`: a ticket source is each ticket plugin's enabled state, whether it can run, and its approved manifest sha256. Any change refetches the lists.
- `PRD-07`: ticket and comment text is data, never loaded or run. `TicketText` renders a deliberate Markdown subset with no HTML parser and no resource loader, and shows a link as its words and its address.
- Only the newest fetch per project lands. A ticket's text lands only while its loading placeholder is still current.
- A list being refetched stays shown (`refreshing`) and never goes back to loading.
- A brief must give something: a toggle that would leave neither text nor any comment is refused.
- `refresh()` clears texts, briefs and parts, since chosen comments may have moved.
- `agentsOn` leaves out sessions that stand beside a chat, and orders the rest by `created_at`, then id. Another agent's branch gets the first free `-2`, `-3`… suffix.

## Side Effects

- IPC via `ask`: `ListIssues`, `ViewIssue`, `ChangeSetting { OnlyMyTickets }` (per project in the group, then a refetch), `ProjectSettings`, `StartFromIssue` (24×80).
- IPC via `bridge.command` (Cleanup.tsx): `CloseIssue`, `RemoveWorktree` (force false or true), `StopSession`.
- `bridge.openUrl(issue.url)` to open the ticket in the browser.
- Store: ticket store fields as above; UI `select`, `show`, `showTerminal`, `keysTo`, `setHints`, `tell`; `whenKnown` selects a new session once the daemon reports it.
- DOM: focus to the project listbox holding the selection; `scrollIntoView` on the comment part moved to.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/screens/tickets`
