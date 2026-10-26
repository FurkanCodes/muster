# flows

## Overview

The window's dialogs and commands. `Flows` is mounted once for the window. It
binds the flow keys from the person's keymap on the screens each one works on:
add or clone a project, new session or worktree, close, stop, restart, tidy,
move a branch, terminal, spaces. It does this only while the daemon link is up;
the help key is bound whatever the link state.
It also adds a folder dropped on the window as a project, and draws the ⌘K
palette or exactly one dialog, chosen by `ui.flow`.

`actions.ts` holds the verbs the TUI has. Each verb works on the current
selection and does one of three things: opens a confirm dialog, sends a command
at once (restart), or puts in the status bar why it cannot act.

## Interface

- `Flow`, `Later` (types.ts) — the dialogs over the window (add-project, clone-project, new-worktree, stop, tidy, move, remove-worktree, remove-project, space-name, remove-space, file-terminal), and work deferred until a space exists.
- `Flows` (Flows.tsx) — binds the flow keys, handles a folder drop, and draws the palette or the active dialog.
- `here`, `Here` (actions.ts) — the selected session, workspace or project on the session lists.
- `addProject`, `addProjectIn`, `addProjectAt` (actions.ts) — the native folder panel (else the folder browser dialog), then `AddProject`, then show and select it.
- `newSession`, `newWorktree`, `cloneProject` (actions.ts) — the project page's composer, the new-worktree dialog, the clone dialog.
- `close`, `stop`, `restart`, `rename`, `tidy`, `moveBranch`, `removeWorktree` (actions.ts) — verbs guarded by the selection.
- `SESSION_LISTS`, `START_SIZE`, `runs`, `plain`, `handsOn` (actions.ts) — the screens with session lists, the first PTY size (100×30), and workspace/session tests.
- `catalogue`, `PaletteItem`, `Ticket`, `Lead` (catalogue.ts) — the palette's items for the typed text, grouped: actions on the selection, anywhere, sessions (those needing you first), tickets, settings.
- `Palette` (Palette.tsx, default export) — the ⌘K palette, in a commands mode or an every-key help mode.
- `keysOf`, `keysIn`, `labelOf`, `KeyAction`, `Key`, `Keyed` (keymap.ts) — an action's chords and key-cap labels from daemon state.
- `makeWorktree`, `startSession`, `openStarted`, `startIn` (start.ts) — `CreateWorktree` and `StartSession`, then open the new session with the keys.
- Dialogs: `AddProject.tsx`, `CloneProject.tsx`, `NewWorktree.tsx`, `BranchDialogs.tsx` (`Tidy`, `Move`), `SessionDialogs.tsx` (`Stop`, `RemoveWorktree`, `RemoveProject`), `SpaceDialogs.tsx` (`SpaceName`, `RemoveSpace`, `FileTerminal`), `Ask.tsx`, `Questions.tsx` (`Question`), `Welcome.tsx`, `Away.tsx`. Helpers: `agents.ts`, `changes.ts`, `folders.ts`, `help.ts`, `match.ts`, `repositories.ts`, `secondAgent.ts`, `useTask.ts`.

## Invariants

- `ARC-25`: dialogs load lazily the first time they are asked for. The palette is loaded eagerly, so keys typed right after ⌘K reach its field.
- `ARC-33`: terminals in no project and spaces are first-class. With no space yet, `Later` carries the work until one is made.
- Nothing here asks the daemon while the link is not up, and a dropped link closes any flow and the palette.
- Work starts in one place only: the project's page.
- A run's container workspace is never a place to start work, tidy or move a branch. The project's checkout is never removed.
- A plain folder (no branch) gets no worktrees, tidy or branch move.
- A palette action presses its bound key first, so the screen can answer it itself, and falls back to the flow's function.

## Side Effects

- IPC: `AddProject` → `ProjectAdded`, `RestartSession`, `CreateWorktree` → `WorktreeCreated`, `StartSession` → `SessionStarted`, `ListIssues` per git project (palette, once the person types).
- Bridge: `pickFolder()`; `onFolderDrop` (unlistened on unmount).
- Store: `openFlow`, `openPalette`, `tell`, `show`, `select`, `showTerminal`, `keysTo("terminal")`, `compose`, `choseAgent`, `foldSidebar`; `whenKnown` waits for a new project or session.
- Mock builds only: `window.__musterUi = app.actions`.
- DOM: synthesised key presses from palette items; `scrollIntoView` on the lit item.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/flows`
