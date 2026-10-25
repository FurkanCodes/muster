# Spaces first: everything lives in a space

*   **Status:** approved by the person on 2026-10-25 ("go with your recommendation"); built on `feature/spaces-first`.
*   **Builds on:** [spaces.md](spaces.md) (spaces and free terminals, protocol 57, store V11).
*   **Rules changed:** `PRODUCT.md` §2 (*Spaces and free terminals*) and §6a; `ARCHITECTURE.md` §2 (*Space*) and `ARC-33`.

## Overview

In [spaces.md](spaces.md) a space is an optional group. A project can be in several spaces or in none, and the list stays a list of projects. This design turns that around. The space is the top level: a project, a folder or a terminal always lives in exactly one space, and the list is grouped by space.

The person said:

> Projects are gone and it is spaces now. When we add a new project, first we have to add a space. Under a space users can just do whatever they want: they can add projects, GitHub repositories, folders, empty terminals, etc. The filter should always display all spaces at first by default.

They chose the three recommendations offered:

1.  Each project and free terminal has **one home space**.
2.  On upgrade, everything is **moved into a space named "Home"**, so nothing disappears.
3.  The default view, *Everything*, is the whole list **grouped under each space's heading**.

## What changes for the person

*   **The list.** *Everything* shows each space as a heading, in the order the spaces were made. Under a space's heading come its projects, in the person's order, each with its worktrees and sessions as before. Then come the space's free terminals, one line each. A space's heading folds as a project's does. The *Terminals in no project* heading goes away, because every free terminal is now under its space.
*   **Choosing a space** shows only that space's projects and terminals, without its heading. The heading menu still switches between spaces, and `]` and `[` still go round them.
*   **Every window opens on *Everything***, in the TUI and in the desktop app. Which space a window shows is still the window's own, but it is no longer kept across restarts.
*   **A space comes first.** With no space yet, adding a project, cloning a repository or starting a terminal first asks for the name of a space to put it in. The new space is made, and then the thing goes into it.
*   **Where something new goes.** It goes into the space of the line selected: the space's heading, a project or anything under it, or a free terminal. With no line selected, it goes into the space shown. In *Everything* with nothing selected, it goes into the first space.
*   **Moving.** A project's menu and a free terminal's menu offer *Move to ‹space›* for each other space, and *Move to a new space…*. *Put in* and *Take out of* go away, because nothing can be in no space.
*   **A space's menu.** A right-click on a space's heading (in the desktop app), or `W` with its heading selected (in the TUI), offers: *Open a folder…*, *Clone from GitHub…*, *Terminal*, *Rename…* and *Remove…*.
*   **Removing a space.** Muster refuses while the space holds anything, and says what: "Work still holds 2 projects and 1 terminal: move or remove them first." So removing a space never removes a project or ends a terminal.
*   **Notes.** The "in Work, Home" note on a line goes away: each thing is in one space, and its heading already says which.

Nothing about where or how anything runs changes. A space still only groups, and every alert still reaches the person whichever space is shown.

## Rules

*   **One home.** The daemon holds this invariant: every project, and every free terminal, is a member of exactly one space. A session in a workspace is in no space itself; it shows where its project does.
*   **The daemon decides a missing space.** `AddProject`, `CloneProject`, `AddProjectHere` and `StartSession` (for a free terminal) take `space: Option<SpaceId>`. The clients always send one. When it is `None` (from the CLI, a plugin or an older script), the daemon uses:
    *   the only space, where there is one;
    *   a space named *Home*, made for it, where there is none;
    *   and otherwise it refuses, naming the spaces: "say which space: Work, Home".
*   **`AddProjectHere`** puts the new project in the terminal's space, unless another is named. The terminal goes under the project and leaves its space.
*   **`FileSession` and `RestartIn`** take the terminal out of its space, because it is now under a project.
*   **`MoveToSpace { space, member }`** replaces `AddToSpace` and `RemoveFromSpace`. It takes the member out of its space and puts it in this one, in one store transaction. Moving it to the space it is already in does nothing.
*   **`DeleteSpace`** is refused with `Conflict` while the space holds anything.

## Protocol (`ARC-09`)

Version 58. `AddToSpace` and `RemoveFromSpace` are removed, which raises the oldest version spoken to 58: an older client is told to update. `MoveToSpace` is new. Nothing else on the wire changes shape. `Space.members` stays, and now holds each member in one space only. Golden frames for v58 go in `crates/muster-core/tests/frozen/v58/`.

## Store

Schema 12 adds no table. Its migration settles what V11 allowed:

1.  A project, or a free terminal, that is in no space goes into the space named *Home*, made if there is none. If a space is already called *Home*, case aside, that one is used.
2.  One that is in several spaces stays in the space made first, and leaves the others.

Both happen in the one migration transaction, so a crash leaves V11 untouched.

## Model

`Model::rows_in(View::All)` lists, for each space in the order made:

*   `Row::Space(id)`;
*   its projects' rows, as `View::Space` lists them;
*   its free terminals, `Row::Session`.

Folding a space hides everything under it. `View::Space(id)` lists that space's rows without the heading. `Row::Terminals` and `Model::in_spaces` go away. The TUI's list and the desktop's `Derived` rows stay one computation (`ARC-31`).

## Clients

*   **TUI.** The space heading is a line: `▾ Work` with its counts, folded by `←` and `→` as a project's line is. `W` opens the spaces menu, and the selected heading's menu when one is selected. `+`, `a` and `g` go into the selected line's space; with no space they ask for a name first. *Move to* replaces *Put in* and *Take out of* in a project's and a terminal's menu. The window opens on *Everything*.
*   **Desktop.** The tree's space heading is a `treeitem` labelled with the space's name. It folds, and its right-click opens the space's menu. The same rules as the TUI apply for adding and moving. `ui.space` is no longer kept in `muster.space`.
*   **CLI.** `muster project add` and `muster term new --free` take `--space NAME`. Without it, the daemon decides as above. `muster space mv <project-or-terminal> <space>` moves one. `muster space rm` is refused while the space holds anything.

## Alternatives considered

*   **Keep several spaces per project.** Rejected by the person: one home is simpler to reason about, and a project's place in the list is then obvious.
*   **Remove a non-empty space, moving what it holds into *Home*.** Rejected: one wrong click would quietly regroup everything. A refusal that names what is left is safer, and costs one move.
*   **Keep the last space chosen across restarts.** Rejected by the person: a window always opens on everything.
*   **A `space` column on projects and sessions instead of `space_members`.** It would fit "one home" more directly, but it would migrate two tables and change `Project` and `Session` on the wire. Keeping members, with the daemon holding the invariant, changes the protocol by one command.

## As built (2026-10-26)

Where the build differs from the plan above, and what is not done yet.

*   **What is in no space for a moment is still listed.** Between two deltas, a project can be added before the space it joins is told. *Everything* lists such lines last, with no heading, so nothing drops out of sight. A space alone never lists them.
*   **A reply can come before its delta.** The reply that a space was made can reach a client before the space itself. So what waited on the new space is done with the space's id from the reply, not by looking the space up. This holds in the TUI (`App::do_later`) and in the desktop app (`doLater`). A TUI unit test forces the reply first; the desktop's tests do not yet.
*   **Notes.** *Needs you* still notes a waiting session's space (`in Work`). A project's line in the list has no note.
*   **The desktop's heading and levels.** A space's heading is drawn as the *Terminals* heading was: a quiet label that folds. Projects keep their place and look under it. Selecting a heading shows nothing beside the list yet; a space's own page is not built.
*   **"Move to a new space…"** keeps the window on *Everything*, in the TUI and in the desktop app. Making a space from the spaces menu still shows it.
*   **The desktop's pretend daemon** (`src/ipc/mock`) keeps the rules too: a Home made where none is asked, moves, and refusing to remove a space that holds anything. Its scenario's projects are in no space, so the browser end-to-end tests keep their rows. Tests that add or clone give it a space first.
*   **Not done yet:**
    *   a space's own page beside the list, when its heading is selected;
    *   ordering spaces by hand (they keep the order they were made in);
    *   the whole test suite, the real-Docker suite and a try on a real VM (`pnpm e2e:app` passed in full: 14 files, 122 tests);
    *   `tui_spaces.rs` failed once in five runs alongside the other TUI files, and passes alone: a race in its keys to look at.

## Verification

*   **Core:** unit tests and round trips for `MoveToSpace`, plus the v58 golden frames.
*   **Store:** migration tests. Something in no space goes to *Home*, one in two spaces keeps the first, and a space already called *Home* is reused.
*   **Daemon:** a `None` space resolves as the rules say; `DeleteSpace` refuses a space that holds anything; a move is one transaction; filing a terminal takes it out of its space.
*   **Model:** `rows_in(View::All)` groups by space, folding a space hides its rows, and a space's view has no heading.
*   **TUI:** the app's unit tests, and `tui_spaces.rs` over the real daemon.
*   **Desktop:** unit tests over the mock, the browser end-to-end tests, and real-app tests. One test makes a space before the first project is added, one moves a project between spaces, one refuses to remove a non-empty space, and one checks that the window opens on everything.
