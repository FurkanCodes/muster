# session

## Overview

The session pane beside the list, and the session actions shared across the
window. `SessionPane` draws a `Page`:

- the header: agent logo, title, and a *Where* line with the branch (to the project page), the +/− diff stat and the clash marker (both to the Changes view), and the pull request (in the browser);
- the actions: shell, a Changes toggle, and the more-menu (also on right-click);
- the body: the lazy Changes board, or the *Needs you* banner over the terminal `Panels`.

## Interface

- `SessionPane` (SessionPane.tsx) — the session page described above.
- `NeedsYouBanner` (NeedsYouBanner.tsx) — "Needs you:" with the question's plain text and a live "since", while the status is `needs_you`.
- `SessionActionsProvider`, `useShownSessionActions`, `useShownSession` (SessionActionsContext.tsx) — one set of session actions for the shown session, so the list and the pane share one rename state and one close dialog. Also draws `CloseSession`.
- `useSessionActions`, `SessionActions`, `CloseSession`, `RenameSession` (useSessionActions.tsx) — rename, save the name, close, confirm close, keep it, open in the browser.
- `useSessionMenu` (useSessionMenu.ts) — the menu's items with their key labels: pin, rename (chats only), checks, review, open a pull request, the pull request in the browser, close.
- `runChecks`, `review`, `openPullRequest`, `browsePullRequest` (byHand.ts) — the TUI's stage tools (c/v/o/b).
- `plainText` (question.ts) — strips OSC/CSI escapes and control characters, keeping line breaks.
- Child: [`./changes/README.md`](./changes/README.md) — the Changes board.

## Invariants

- `ARC-33`: a terminal in no project (`workspace === null`) has no undo and no Changes view. Its pane says so, and its menu is pin, *Put under a project…*, move to a space, and close.
- Once shown, the Changes view holds the keys (`KeysHeld`); a key it does not answer never reaches the list.
- Closing a running session asks first and sends `StopSession`. The session is removed only once it has ended; an ended session is removed at once.
- Rename is offered only for chats, and an empty name is not sent.
- The banner never offers the agent's choices; the person answers in the terminal.
- `useShownSessionActions` is used only inside `SessionActionsProvider`.

## Side Effects

- IPC: `RenameSession`, `StopSession`, `RemoveSession`; `CheckWorkspace` and `ReviewWorkspace` → `SessionStarted`; `OpenPullRequest` → `PullRequestOpened`; `bridge.openUrl(pr.url)`.
- Daemon store: a subscription waits for a closed session to end, then unsubscribes. `whenKnown` opens a started session once it is known.
- Store: `keysTo`, `showPane`, `select`, `show`, `showTerminal`, `openMenuAt`, `openFlow({kind: "file-terminal"})`, `clearNote`, `tell`.
- Layout: `keepLayout(...)` to pin or unpin; `toggleFullScreen`.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/session`
