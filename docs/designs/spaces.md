# Spaces and free terminals

*   **Author:** Furkan
*   **Status:** Built on `feature/spaces` (2026-10-25), not yet merged
*   **Date:** 2026-10-25

--------------------------------------------------------------------------------

## Overview

Muster lists projects in one flat list. A project is a git repository or a plain folder. Under it are its workspaces (worktrees, the checkout, a container), and under those its sessions (agents and terminals). Every session belongs to a workspace, so every terminal belongs to a project.

This design adds two things:

1. **Spaces.** A space is a named group the person makes. It holds projects and terminals, and one project can be in several spaces. Choosing a space shows only its members in the sidebar; an **All** view shows everything. A space is only a label. It changes nothing about where or how anything runs.
2. **Free terminals.** A terminal can be started with no project and no space. It opens in the home folder and is listed under **Terminals** in All. The person can file it into a space later, or put it under a project.

Both come in one release, in the daemon, the desktop app, the terminal UI (TUI) and the CLI. The predecessor brief is [concepts_brief.md](../../concepts_brief.md).

## The Problem

* **No way to group projects.** The only control over the project list is its order. Work for different purposes can't be kept apart, for example personal and client work, or a VM's work and the Mac's. Today the only option is to scroll past or fold what isn't wanted.
* **No terminal outside a project.** To open a shell for a quick job, such as checking something on a VM over `muster ssh`, the person first has to add or pick a project. The terminal is then filed under a project it has nothing to do with.

**Counter-argument.** No single incident has been named. The project order, folding and colours each project gets already give some control, and tags on projects with a filter would cover most of the grouping.

Tags were rejected because they can't hold a free terminal that is filed later (see the brief, Analysis §1). The cost of this design is high, because `Session.workspace` is read across nearly every crate and screen, so the Goals below are kept to what the person asked for.

**Solution in one paragraph.** The daemon keeps spaces in its store, each as a name plus members (projects and terminals). A session's place becomes either *in a workspace* or *free*. A free terminal runs in the home folder. The person can file it into a space by adding it to that space's list, which touches nothing in the shell. It can also be put under a project:
* in place, when its shell is already in that project's folder;
* otherwise by starting a new terminal there.

Muster never types into the person's shell. Each window keeps its own current space, and the sidebar shows only that space's members. Alerts ignore spaces.

## Goals

*   **G1 — Spaces.** The person can create, rename and delete spaces, add projects and terminals to them, and remove members. One project can be in several spaces. Changes from one client appear in every other client.
*   **G2 — Space filter.** In the desktop app and the TUI, choosing a space shows only its members in the sidebar. All shows everything. The CLI's `muster list` takes `--space`.
*   **G3 — New items join the current space.** A project added, or a terminal started, from a window with a chosen space becomes a member of that space. With no space chosen it belongs to none and shows in All.
*   **G4 — Free terminals.** A terminal can be started with no project. It runs in the home folder of the machine whose daemon started it. A daemon restart treats it exactly as it treats a project's terminal today. A restart in place (`muster reload`, an upgrade) hands it over still running. A daemon that stopped or crashed leaves it ended, because a shell has no conversation to resume (`supervisor/recovery.rs`, `resumable`). In both cases it keeps its line, name and spaces.
*   **G5 — Under a project.** A free terminal can be put under a project:
    *   **(a) In place:** when its shell's current folder is inside one of that project's workspaces, the same shell keeps running.
    *   **(b) Restart:** otherwise it becomes a new terminal in that project. The person is first told which program will end.
    *   **(c) Add the folder:** when the shell is in a folder that isn't a project yet, the person can add that folder as a project and file the terminal under it in one step, keeping the same shell.
*   **G6 — Alerts are unchanged.** "Needs you", the notch and Mac notifications cover every session whatever space is chosen. Each "needs you" item says which space it is in.
*   **G7 — The upgrade moves nothing.** After upgrading, every existing project appears in All, no space exists, and no session changes.
*   **G8 — Both interfaces together.** The desktop app and the TUI offer the same space and free-terminal actions in the same release (PRODUCT.md §6), each reachable by key and by mouse (ARC-32).

## Non-Goals

*   **Muster typing into the person's shell.** No `cd`, no keys, for any reason.
*   **A button that starts an agent with no project.** An agent the person types into a free terminal is shown as an agent, with a "no project: no undo, no changes view" note. It is not blocked.
*   **Spaces that change where things run.** That includes a space tied to a machine.
*   **Settings for each space.** No muting for each space, no space colours, no settings for each space.
*   **A project order for each space.** The one project order stays the only one.
*   **A space spanning several daemons.** Each daemon, local or `--on vm`, has its own spaces and free terminals.
*   **Moving agents.** An agent running in a workspace is never moved to another, as today.
*   **Keeping v0.7.0 working on the upgraded store.** Going back means restoring the backup Muster takes before upgrading the store.

## Open Questions

No question is open. Answered on 2026-10-25 and carried into the Technical Plan:

*   **A free terminal can be in several spaces,** like a project.
*   **Filing under a project with worktrees** goes to the workspace whose folder holds the shell's current folder, the deepest if several do. The restart path uses the project's checkout.
*   **A free terminal's folder is read only when its menu opens.** Its line in the list shows no folder (the terminal itself shows it).
*   **Limits:** at most 64 spaces. A name is 1–40 characters on one line, unique, with surrounding spaces trimmed.
*   **Dismissing an ended free terminal** removes it as an ended session is removed today, and takes it out of every space in the same step.
*   **After a daemon restart,** free terminals do what a project's terminal does today (G4).
*   **The oldest protocol spoken rises from 54 to 57.** Older clients and VM daemons are told to update (Technical Plan, "Protocol").
*   **A plugin's panel can't be opened beside a free terminal.** Muster refuses and says why.

## Critical User Journeys (CUJs)

### CUJ 1: Group projects into spaces

*   **Given**: Muster just upgraded. Projects A, B and C exist, and no space exists.
*   **When**: The person creates a space "Work", adds A and B to it, then creates a space "Home" and adds B and C.
*   **Then**:
    *   Choosing Work shows A and B; choosing Home shows B and C; All shows A, B and C once each, with B noted "in Work, Home".
    *   The other client (TUI or desktop) shows the same spaces with no reload.
    *   Alerts from C still arrive while Work is chosen, labelled "Home".

### CUJ 2: A free terminal for a quick job

*   **Given**: No space is chosen.
*   **When**: The person starts a new terminal without choosing a project, then runs `muster ssh vm` in it.
*   **Then**: The terminal opens in the home folder and appears under **Terminals** in All. Its menu offers "Add to space…". Adding it to a space "VM" lists it there, with the shell and the ssh connection untouched.

### CUJ 3: File a free terminal under the project it has moved into

*   **Given**: A free terminal whose shell the person has `cd`'d into project A's checkout folder.
*   **When**: The person chooses "File under A" from its menu.
*   **Then**: The terminal appears under A's checkout with the same shell, scrollback and running program. It leaves any spaces it was in directly, and shows wherever A shows. Its tabs and pinned panels still show it.

### CUJ 4: File a busy free terminal under a project it isn't in

*   **Given**: A free terminal in `~/Downloads` running `npm run dev`.
*   **When**: The person chooses "Start a new terminal in A".
*   **Then**:
    *   A dialog says that `npm run dev` will end and that background jobs aren't listed.
    *   If confirmed, a new terminal under the same name starts in A's checkout, shown where the old one was, and the old one ends.
    *   If cancelled, nothing changes.

### CUJ 5: Make a project from where a free terminal is

*   **Given**: A free terminal whose shell is in `~/code/tool`, which isn't a project.
*   **When**: The person chooses "Add ~/code/tool as a project and file this terminal under it".
*   **Then**: Project "tool" is added (a git repository or a plain folder, as `add_project` decides), and the terminal moves under its checkout with the same shell. If the shell's folder changed before the step ran, nothing happens and the person is told why.

### CUJ 6: A space filters, and new items join it

*   **Given**: The space Work is chosen in the desktop app, while the TUI has no space chosen.
*   **When**: The person adds project D and starts a free terminal from the desktop app, then starts a free terminal from the TUI.
*   **Then**: D and the desktop's terminal are members of Work. The TUI's terminal is in no space and shows in All only.

## Technical Plan (Architecture)

### Where each part lives

```text
                 muster-core (types, no IO)
   Session.place: SessionPlace { Workspace(id) | Free }     Space { id, name, members }
   Command: spaces, StartSession{place, space}, ReadPlace, FileSession,
            AddProjectHere, RestartIn          Delta: SpaceUpserted / SpaceRemoved
                         │
   ┌─────────────────────┼──────────────────────────────┐
   ▼                     ▼                              ▼
muster-daemon        muster-model (pure)            muster-client
 store V11            rows_in(View)  ◄── one place      (since() gates)
 supervisor/spaces    View = All | Space(id)               │
 supervisor/free      spaces_of(member)          ┌────────┼─────────┐
 fronts: cwd(pid)                                 ▼        ▼         ▼
 (Processes trait)                              TUI   desktop link   CLI
                                                       (Derived.space_rows)
                                                           ▼
                                                        webview
```

*   **Types and wire (muster-core).**
    *   A session's place becomes a typed value: in a workspace, or free.
    *   Spaces are a new entity.
    *   New commands and two new deltas carry them.
    *   No IO (ARC-01).
*   **Owner (muster-daemon).**
    *   **Store:** spaces, members, and a session's place (store V11).
    *   **Supervisor:** decides every rule, such as membership leaving with its member, the folder check before filing, and refusing agents with no project (ARC-02).
    *   **Shell folder:** read through the `Processes` trait (ARC-03).
*   **What is shown (muster-model).**
    *   One pure function answers "which lines does this view show". The TUI uses it directly. The desktop's Tauri link uses it and sends its result as `Derived` (ARC-24, ARC-31).
    *   The webview draws lines and never filters.
*   **Current space.** Each client keeps its own, never the daemon:
    *   **TUI:** its `App` state.
    *   **Desktop:** each window's `ui` store.
    *   **CLI:** a `--space` flag.

### Data

*   **`SessionPlace`** (muster-core `entities.rs`) replaces `Session.workspace`:
    *   `Workspace(WorkspaceId)`: where every session is today;
    *   `Free`: a terminal with no project, run in the home folder of its daemon's machine. The path is looked up at start, never stored.

    `Session::workspace() -> Option<WorkspaceId>` serves the many places that only need "which workspace, if any". Every `match` on `SessionPlace` names both cases, so the compiler lists each place that must decide. About 35 Rust files and 30 webview files read a session's workspace today.
*   **`Space`** (new, muster-core):

    | Field | Meaning |
    |---|---|
    | `id: SpaceId` | Never changes; the name is only a label. |
    | `name: String` | 1–40 characters, one line, unique, trimmed. |
    | `members: Vec<Member>` | `Member::Project(ProjectId)` or `Member::Session(SessionId)`; no repeats. |
    | `created_at` | Spaces are listed in the order they were made. |

*   **Store V11** (`muster-daemon/src/store/migrations.rs`):
    *   **Rebuild `sessions`** with `workspace_id` allowed to be empty. SQLite can't drop `NOT NULL` in place, so the migration creates a new table, copies the rows, drops the old one and renames the new one, with foreign keys off for the copy. `status_changes` keeps its link to `sessions`.
    *   **Add `spaces`:** `id`, `name` unique, `created_at`.
    *   **Add `space_members`:** `space_id` (removed with its space), `kind` (`project` or `session`), `member_id`, keyed on all three.
    *   **Membership leaves with its member.** Removing a project or session also deletes its memberships in the same transaction, including the memberships of a removed project's sessions. A foreign key can't point at two tables, so the store does this in its remove methods.
    *   **Backup.** `Store::open` already backs up the database before migrating (`store/backups.rs`). The log names the backup's path.
*   **Snapshot** gains `spaces: Vec<Space>`, read as empty when missing.

### Protocol (ARC-09)

| Frame | Shape | What it does |
|---|---|---|
| `Command::CreateSpace { name }` | → `Reply::SpaceCreated { id }` | Refused: name taken, empty, too long, 64 spaces already. |
| `Command::RenameSpace { space, name }` | → `Ok` | Same name rules. |
| `Command::DeleteSpace { space }` | → `Ok` | Removes the group only. Its members stay. |
| `Command::AddToSpace { space, member }` / `RemoveFromSpace` | → `Ok` | Adding again does nothing. A session filed under a project can't be added; its project can. |
| `Command::StartSession { place, space, … }` | → `Reply::SessionStarted` | Today's command, with `place` replacing `workspace` and an optional `space` to join. `Free` with an agent harness is refused: "an agent starts in a project". |
| `Command::AddProject { root, name, space }` | → `Reply::ProjectAdded` | Today's command, plus the optional `space`. |
| `Command::ReadPlace { session }` | → `Reply::Place { folder, workspace, front }` | The shell's current folder, the workspace that holds it (if any), and the program in front. Read only when a session's menu opens. `folder: None` when it can't be read. |
| `Command::FileSession { session, workspace }` | → `Ok` | A free terminal moves in place. The daemon reads the folder again and refuses if it isn't inside `workspace` or can't be read. It leaves its spaces. An agent running in it (ARC-28) moves with it, because its folder stays the same. |
| `Command::AddProjectHere { session, space }` | → `Reply::ProjectAdded` | Reads the folder, refuses the home folder or one above it, adds the project (as `AddProject` does), then files the session under its checkout. All in one step, so nothing is half done. |
| `Command::RestartIn { session, workspace }` | → `Reply::SessionStarted` | Today's `muster move`, done by the daemon: start a terminal under the same name in `workspace`, point every client showing the old one at the new one (`Switch`), stop and remove the old one. Refused while an agent runs in it, because that would end its conversation. The CLI, TUI and desktop app all use it. |
| `Delta::SpaceUpserted(Space)` / `SpaceRemoved { id }` | — | One per change. A session moved is the existing `SessionUpserted`. |

*   **Version 57.** Each new frame has its `ClientFrame::since()` entry, and the new deltas a `Delta::since`. There is a frozen handshake set `tests/frozen/v57/` and round trips for every frame.
*   **The oldest version spoken goes from 54 to 57.** `Session`'s shape changes, so a 54–56 client can't read the sessions list. ARC-09 says "removals raise the minimum".
    *   The other way is to keep `workspace` on the wire for old clients and never send them free sessions. That needs a second session shape and a filter in the daemon's pump, kept until the minimum rises anyway.
    *   Raising the minimum means a VM still on v0.7.0 must be updated (`muster ssh vm --install`) before `--on vm` works.

### Daemon

*   **`supervisor/spaces.rs`** (new): the space commands. Each command does three things in order:
    *   checks the name and limits;
    *   writes the store;
    *   sends one delta.

    Removing a project (`supervisor/projects.rs`) or a session also sends `SpaceUpserted` for each space that lost a member.
*   **`supervisor/free.rs`** (new): `ReadPlace`, `FileSession`, `AddProjectHere` and `RestartIn`.
    *   **Choosing the workspace:** filing chooses the workspace whose host folder holds the shell's folder, the deepest if several do. The folder test compares paths with symlinks resolved, so `/tmp` and `/private/tmp` match on a Mac.
*   **`supervisor/sessions.rs`:**
    *   **`start_session` and `place_for`:** they take a `SessionPlace`. `Free` gives the harness place `Place::Host(home)`, from the daemon's `HOME`.
    *   **`session_path`:** for a free session it gives the home folder. Shells in a free place get the same variables and hooks as any shell, so `muster exec` agents in them (ARC-28) are heard.
*   **Features that need a workspace.** Changes, timeline, clashes, tidy, worktrees, undo and the plugin context (`changes.rs`, `timeline.rs`, `clashes.rs`, `tidy.rs`, `worktrees.rs`, `panels.rs`, `events.rs`) take `SessionPlace::Workspace` only. For a free session they do nothing, or refuse with a reason. Opening a panel beside a free terminal is refused. Plugin events already carry an optional project (`muster-core/src/plugins/events.rs`); for a free session it is empty.
*   **Recovery and hand-off are unchanged.** The store keeps the place. A restart in place hands a free shell over running. A daemon that stopped ends it, as any shell.
*   **`fronts.rs`:**
    *   **New method:** `Processes` gets `cwd(pid) -> Option<PathBuf>`. `SystemProcesses` reads it with sysinfo's `with_cwd(UpdateKind::Always)`: libproc on a Mac, `/proc/<pid>/cwd` on Linux.
    *   **Which process:** it reads the shell's own process (`PtyHandle::pid()`), never the program in front.
    *   **When:** only when `ReadPlace`, `FileSession` or `AddProjectHere` asks. The watcher's tick never reads it.
    *   **Tests:** a table stands in for the trait.
*   **Logging.** Every space command, every refusal (with its reason), every unreadable folder, and the V11 migration with its backup path are logged with `tracing`. No folder's contents or terminal output is logged.

### Model and clients

*   **muster-model:**
    *   **Storage and updates:** it keeps `spaces` and applies the two deltas. Removing a member it has never seen does nothing.
    *   **`rows_in(view)`:**
        *   **All:** today's `rows()`, then a `Row::Terminals` heading and the free sessions by start time.
        *   **A space:** its projects in the person's project order, each with its workspaces and sessions as today, then a Terminals heading with its free terminals.
    *   **`spaces_of(member)`:** the spaces a project or session is in, for the "in Work, Home" note.
    *   **`needs_you()`** is unchanged, so alerts ignore spaces (G6).
*   **TUI** (`muster-tui`):
    *   **Choosing a space:** a current space in `App`, shown in the top bar. Next and previous space get one key each, set in `muster_core::keys`. "Go to space…", "New space", "Rename space" and "Delete space" are in the palette.
    *   **The list:** it draws `rows_in(view)`. A session's menu gains "Add to space…", "Remove from <space>", "File under <project>", "Add <folder> as a project" and "Start a new terminal in <project>…". Each is shown only when it can work, using `ReadPlace` for the folder ones.
    *   **Keys and mouse:** every action is in `Action::ALL` with a mouse path (ARC-32).
    *   **New files:** new code goes in `app/spaces.rs` and `view/spaces.rs` (ARC-08; `app.rs` and `view/sessions.rs` are already 771 and 959 lines).
*   **Desktop.**
    *   **Link (`src-tauri`):** `Derived` gains `space_rows: BTreeMap<SpaceId, Vec<Row>>`, from `rows_in` for each space, beside today's `rows` for All. Each window picks the list for its current space, so the webview never filters.
    *   **Webview:**
        *   **Switcher:** a space switcher at the top of `Sidebar`, built on Base UI's menu. It shows "New space" alone until a space exists.
        *   **Current space:** the window's `ui` store holds it. It is remembered in local storage for that window as a convenience, and falls back to All when the space is gone.
        *   **The list:** a Terminals section, the same session menu items as the TUI, and the restart dialog that names the program that ends.
        *   **Components:** all from `src/ui` (ARC-26).
*   **CLI** (`crates/muster/src/verbs`):
    *   `muster space add|rename|rm NAME`;
    *   `muster list spaces`;
    *   `--space NAME` on `muster list` and `muster term new`;
    *   `muster term new --free`;
    *   `muster move` calls `RestartIn` instead of doing the steps itself.

## Alternatives Considered

### Alternative A: Tags on projects, plus a sidebar filter

*   **Pros**:
    *   It is the smallest change: a list of tags on each project, kept beside the project order.
    *   It needs no new level and no change to `Session`.
    *   It allows a project in several groups.
*   **Cons**:
    *   It can't hold a terminal that isn't in a project.
    *   "Choose a group" becomes "filter by a tag", which is a weaker idea of where one's work is.
*   **Reason for Rejection**: the person wants free terminals that can be filed later. A tag has nothing to attach to there.

### Alternative B: One group name per project, shown as foldable headings

*   **Pros**: even smaller than tags. It reuses folding.
*   **Cons**: a project can be in only one group, and no terminals can be grouped.
*   **Reason for Rejection**: the person asked for a project in several spaces, and for free terminals.

### Alternative C: Spaces kept in `settings.toml`, like the project order

*   **Pros**: the person can edit them by hand, and the store doesn't change.
*   **Cons**:
    *   `PreferenceChange::ProjectOrder` replaces the whole list, so the TUI and the desktop app would overwrite each other's edits.
    *   The settings reader rejects tables it doesn't know (`deny_unknown_fields`), so an older Muster would stop loading settings.
    *   Session members have no lasting id there.
*   **Reason for Rejection**: lost edits, and a broken rollback of settings. The store loses no rollback that settings would have kept, because free terminals change the store anyway.

### Alternative D: Spaces in a separate `spaces.toml`

*   **Pros**: an older Muster never reads it.
*   **Cons**:
    *   State is split between the store and a file. A session's membership and the session itself could disagree after a crash.
    *   Edits still need a writer and a lock.
*   **Reason for Rejection**: free terminals need store V11 regardless. One store keeps membership and its member in the same transaction.

### Alternative E: A hidden home-folder project for free terminals

*   **Pros**: `Session.workspace` stays as it is, and there is less code at first.
*   **Cons**:
    *   Every reader of the project list must skip it. The second review counted them across the CLI, TUI, desktop app, notch, daemon and plugins.
    *   `projects.root` is unique, so the person could never add their home folder as a project.
    *   Changes and git scans would cover the whole home folder.
    *   It goes against PRD-04, which says a plain folder *is* a project.
*   **Reason for Rejection**: a convention every future feature must remember, against a typed place the compiler checks once.

### Alternative F: A folder Muster makes for each space

*   **Pros**: shells and chats with no project get a home, with no change to `Session`.
*   **Cons**:
    *   The space then owns a folder on disk. Renaming or deleting a space raises questions about that folder, and names can collide.
    *   It contradicts "a space only groups".
*   **Reason for Rejection**: the first review raised all of the above. Free terminals in the home folder replace it.

### Alternative G: Muster types `cd <folder>` into an idle shell to move it under a project

*   **Pros**: any free terminal can be put under any project without restarting.
*   **Cons**:
    *   Muster can only see that no program runs in front of the shell (`PtyHandle::foreground()`), not that the line is empty. A half-typed command, a `read` prompt or a nested shell would be corrupted.
    *   It writes to the person's shell history.
    *   There is no prompt marker to rely on: OSC 6973 marks only the end of an agent `muster exec` started.
*   **Reason for Rejection**: unsafe input into the person's shell. Filing in place covers a shell already in the folder, and the restart path covers the rest.

### Alternative H: Keep old clients working with two session shapes on the wire

*   **Pros**: clients and VM daemons on protocol 54–56 keep working.
*   **Cons**:
    *   The daemon's pump would need a second `Session` shape, and a filter that hides free sessions from old clients.
    *   Both would stay until the oldest protocol rises anyway.
*   **Reason for Rejection**: extra code only for a short window. Muster's own `muster ssh --install` updates a VM in one step.

### Alternative I: The daemon keeps the current space

*   **Pros**: every client would show the same space.
*   **Cons**:
    *   Two windows would fight over one setting.
    *   The CLI and `muster ssh` would land new things in a space the person can't see.
*   **Reason for Rejection**: the current space is how one window is viewed, not state Muster keeps for everyone.

### Alternative J: Ship the desktop app first, the TUI later

*   **Pros**: a smaller first release, as the TUI has no filter to build on.
*   **Cons**: PRODUCT.md §6 says both interfaces say the same things. The TUI would show free terminals and spaces it can't choose between.
*   **Reason for Rejection**: the person chose both together.

--------------------------------------------------------------------------------

## Non-Functional Requirements

*   **Security and privacy.**
    *   **Home-folder agents.** A free terminal runs in the home folder, and an agent typed into it can read and change anything there. Muster doesn't block this, as it doesn't block the person's own terminal. It marks the session "no project: no undo, no changes view".
    *   **Shell folders.** Muster reads a shell's folder only when asked, and never the folder's contents. Nothing typed or shown in a terminal is logged (ARC-20, PRD-06).
*   **Performance.**
    *   **Folder reads.** A read is one sysinfo refresh for one process. It happens only when a menu opens or a filing command runs; nothing polls.
    *   **`space_rows`.** The desktop gets one list of lines per space: at most 64 lists, each no longer than today's `rows`.
*   **Rollout and rollback.**
    *   **One release.** Everything ships together: store V11, protocol 57, and 57 as the oldest protocol spoken.
    *   **Upgrading.** The release notes say three things: older clients and VM daemons must update; going back to v0.7.0 means restoring the store backup the upgrade made (its path is in the daemon's log); and nothing is moved on upgrade.
    *   **No feature flag.** With no space made and no free terminal started, everything looks and works as before.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

Rules followed throughout:

*   A rule the daemon decides is never decided again in a client (ARC-02, ARC-24).
*   Each new file stays well under 1,000 lines (ARC-08).
*   Every changed type the webview reads is regenerated with `pnpm bindings`.
*   The test names cite the rule they prove (AGENTS.md).

### Target Files to Create/Modify

#### Core types and wire (`crates/muster-core`)

| File | Change | Why |
|---|---|---|
| `src/ids.rs` (Modify) | Add `SpaceId`, a ULID like the others. | A space's lasting identity. |
| `src/entities.rs` (Modify) | Add `SessionPlace { Workspace(WorkspaceId), Free }`. Replace `Session.workspace` with `Session.place`, and add `Session::workspace() -> Option<WorkspaceId>` and `Session::is_free()`. | A free terminal as a typed state the compiler checks. |
| `src/spaces.rs` (Create) | `Space { id, name, members, created_at }` and `Member { Project(ProjectId), Session(SessionId) }`. A pure `check_name(name, others) -> Result<String, NameRefusal>` (trims; 1–40 characters, one line, unique); `MAX_SPACES = 64`. | One place for the name rules, used by the daemon and shown by clients. |
| `src/lib.rs` (Modify) | `pub mod spaces;` and re-exports. | |
| `src/protocol.rs` (Modify) | `PROTOCOL_VERSION = 57`, `MIN_PROTOCOL_VERSION = 57`. New `Command`s: `CreateSpace`, `RenameSpace`, `DeleteSpace`, `AddToSpace`, `RemoveFromSpace`, `ReadPlace`, `FileSession`, `AddProjectHere`, `RestartIn`. `StartSession` takes `place: SessionPlace` and `space: Option<SpaceId>`; `AddProject` gains `space`. New `Reply`s: `SpaceCreated { id }`, `Place { folder: Option<PathBuf>, workspace: Option<WorkspaceId>, front: Option<String> }`. A `since()` entry of 57 for each new frame. Refusals use the existing `ErrorCode`s: `Conflict` for a name taken, `BadRequest` for a bad name or too many spaces, `NotFound` for a missing space (ARC-07). | The wire for every new action (ARC-09). |
| `src/protocol/state.rs` (Modify) | `Snapshot.spaces: Vec<Space>` with `#[serde(default)]`. `Delta::SpaceUpserted(Space)`, `Delta::SpaceRemoved { id }`, each with a `Delta::since` of 57. | Every client sees spaces and their changes. |
| `src/plugins/events.rs` (Modify) | Where an event takes its workspace from a session, take `session.workspace()` and leave the project empty for a free session. | Plugins are told nothing wrong about a free terminal. |
| `src/plugins/events/tests.rs` (Modify) | An event from a free terminal has no project or workspace. | |
| `src/keys.rs` (Modify) | `Action::NextSpace` and `Action::PreviousSpace`, with default keys, plus palette actions for New, Rename, Delete and Go to space. Each is in `Action::ALL`. | Keys for the TUI and the desktop, read from one table (ARC-32). |
| `src/keys/tests.rs` (Modify) | The new actions have mouse paths and no default clashes with another. | |
| `tests/protocol_round_trip.rs` (Modify) | A round trip of every new command, reply and delta, and of `Session` with both places. | ARC-09. |
| `tests/frozen/v57/` (Create) | The frozen handshake set, written by its ignored test. | ARC-09: an old client is always told to update. |
| `tests/frozen_protocol.rs` (Modify) | Read v57's set. | |
| `tests/variants/mod.rs` (Modify) | The new variants, where this file lists every variant for the tests. | |

#### Daemon (`crates/muster-daemon`)

| File | Change | Why |
|---|---|---|
| `src/store/migrations.rs` (Modify) | V11:<br>• rebuild `sessions` with `workspace_id` allowed to be empty: new table, copy, drop, rename, with foreign keys off for the copy and checked after;<br>• add `spaces` (`id`, `name UNIQUE`, `created_at`);<br>• add `space_members` (`space_id` removed with its space, `kind CHECK IN ('project','session')`, `member_id`, primary key on all three). | A free session's empty workspace; spaces kept with their members. |
| `src/store.rs` (Modify) | `insert_session` writes the place. `spaces()`, `insert_space`, `rename_space`, `delete_space`, `add_member`, `remove_member`, `set_session_place(session, place)`. `remove_session` and `remove_project` delete the memberships of what they remove (a removed project's sessions included) in the same transaction. The snapshot reads spaces. | One transaction keeps membership and member together. |
| `src/store/rows.rs` (Modify) | Read and write `SessionPlace` (an empty `workspace_id` is `Free`), `Space` and `Member`. A bad value is `StoreError::Invalid`, as today. | |
| `src/store/tests.rs` (Modify) | Tests:<br>• every older schema reaches V11 with sessions intact;<br>• a free session survives reopening;<br>• removing a project or session removes its memberships;<br>• a space's name is unique;<br>• the V11 copy is made before migrating (PRD-09). | |
| `src/fronts.rs` (Modify) | `Processes::cwd(pid) -> Option<PathBuf>`. `SystemProcesses` uses `ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always)`. A test reads this process's folder. | The shell's folder, behind the trait (ARC-03). |
| `src/supervisor.rs` (Modify) | Route the new commands to `spaces.rs` and `free.rs`. Pass `place` and `space` through `StartSession` and `AddProject`. | |
| `src/supervisor/spaces.rs` (Create) | Create, rename, delete, add and remove, each checking the name and limits, writing the store and sending one delta. `forget_member(member)` is called when a project or session leaves, and sends `SpaceUpserted` for each space it was in. `join(space, member)` is used by the start and add commands. | The space rules in one owner (ARC-02). |
| `src/supervisor/free.rs` (Create) | `read_place`, `file_session`, `add_project_here` and `restart_in`, plus the pure `holding_workspace(folder, workspaces)`, which resolves symlinks and picks the deepest match. `file_session`:<br>• refuses a session that isn't free or whose folder isn't inside the workspace;<br>• sets the place and leaves its spaces;<br>• sends `SessionUpserted`.<br><br>`restart_in` is today's `muster move` steps, done in the daemon; it refuses while an agent runs. `add_project_here` refuses the home folder and any folder above it. | Putting a terminal under a project (G5). |
| `src/supervisor/sessions.rs` (Modify) | `start_session` and `place_for` take a `SessionPlace`; `Free` with an agent harness is refused, and a shell is placed at `Place::Host(home)`. `session_path` gives home for a free session. `add_session_tied` stores the place, and joins `space` where given. | Free terminals start (G4). |
| `src/supervisor/projects.rs` (Modify) | `add_project` joins `space` where given. `remove_project` calls `forget_member` for the project and its sessions. | G3, and membership leaves with its member. |
| `src/supervisor/recovery.rs` (Modify) | Read the place through `session.workspace()`. A free session is not in a container, so it waits its turn and ends as a shell does. | |
| `src/supervisor/panels.rs` (Modify) | Opening a panel beside a free terminal is refused, saying why. The panel context takes `session.workspace()`. | The answered panel question. |
| `src/supervisor/changes.rs`, `timeline.rs`, `clashes.rs`, `tidy.rs`, `worktrees.rs`, `containers.rs`, `events.rs`, `by_hand.rs` (Modify) | Each `session.workspace` becomes `session.workspace()` with the free case decided: nothing to do, or a refusal with a reason. | The compiler lists them. Each states what a free session means for it. |
| `src/clashes.rs` (Modify) | The same, for its one read. | |
| `src/supervisor/fronts.rs` (Modify) | Agents found in a free terminal (ARC-28) are recorded as for any terminal. Nothing here needs a workspace. Check this, and add a test. | A free terminal running `claude` is an agent. |
| `src/clashes_tests.rs`, `src/take_over_tests.rs`, `src/panel_tests.rs`, `src/supervisor/worktrees_tests.rs` (Modify) | Build sessions with `SessionPlace::Workspace`. | Their fixtures use the old field. |

#### Model (`crates/muster-model`)

| File | Change | Why |
|---|---|---|
| `src/lib.rs` (Modify) | Keep `spaces` and apply the two deltas. `Row::Terminals` (a heading). `View { All, Space(SpaceId) }`. `rows_in(view)`. `rows()` stays, as `rows_in(View::All)`. `spaces_of(member)`, `space(id)`, `spaces()`. Every `.workspace` read goes through `workspace()`; `under_its_chat` and `sessions_of` skip free sessions. | The one place that decides what each view shows (ARC-31). |
| `src/spaces.rs` (Create) | `rows_in` and the membership look-ups, split out of `lib.rs` (761 lines). Tests:<br>• a space shows its projects in the project order, then its free terminals by start time;<br>• All lists everything once;<br>• a project in two spaces shows in both;<br>• a filed terminal shows with its project;<br>• a deleted space's view is empty. | ARC-08. |

#### Client library (`crates/muster-client`)

| File | Change | Why |
|---|---|---|
| `src/connection.rs` (Modify) | Nothing new beyond `since()`; the version tests now expect a 57 minimum. | `ClientError::DaemonTooOld` names `muster reload` for a 56 daemon. |

#### Terminal UI (`crates/muster-tui`)

| File | Change | Why |
|---|---|---|
| `src/app.rs` (Modify) | `space: Option<SpaceId>` in `App`. When the space is gone, fall back to All. | Each TUI keeps its own current space. |
| `src/app/spaces.rs` (Create) | Next and previous space, the dialogs (new, rename, delete with confirmation), "Add to space…", "Remove from <space>". These send the space commands. | ARC-08: `app.rs` is 771 lines. |
| `src/app/spaces_tests.rs` (Create) | Choosing, cycling, falling back to All, and new items joining the current space. | |
| `src/app/free.rs` (Create) | Starting a free terminal. The session menu's "File under <project>", "Add <folder> as a project" and "Start a new terminal in <project>…", each offered from `ReadPlace`'s answer. The restart dialog names `front` and says background jobs aren't listed. | G5. |
| `src/app/free_tests.rs` (Create) | The items are offered only when they can work. A refusal is shown. The restart is asked first. | |
| `src/app/menu.rs`, `src/app/actions.rs`, `src/app/events.rs`, `src/app/shell.rs` (Modify) | The new menu items and actions. `StartSession` with `place` and `space`. | |
| `src/app/palette_tests.rs` (Modify) | The space actions are in the palette. | |
| `src/app/project_order.rs`, `fold.rs`, `prefix.rs`, `plugin_panel.rs`, `link.rs`, `changes.rs` (Modify) | `session.workspace()` where they read it. A free session has no changes pane. | |
| `src/view/spaces.rs` (Create) | The Terminals heading, a free terminal's line, and the "in Work" note. | ARC-08: `view/sessions.rs` is 959 lines. |
| `src/view/sessions.rs` (Modify) | Draw `rows_in(view)` and hand the Terminals rows to `view/spaces.rs`. | |
| `src/view/top_bar.rs` (Modify) | Show the current space's name, or nothing for All. | |
| `src/view/terminal.rs`, `src/view/tickets.rs` (Modify) | `session.workspace()`. | |
| `src/view/dialogs.rs` (Modify) | The space dialogs and the restart dialog. | |

#### Notch (`crates/muster-notch`)

| File | Change | Why |
|---|---|---|
| `src/news.rs`, `src/mirror.rs`, `src/board.rs`, `src/trouble.rs` (Modify) | `session.workspace()`. A free session shows under no project. | Alerts cover every session (G6). |

#### CLI (`crates/muster`)

| File | Change | Why |
|---|---|---|
| `src/main.rs` (Modify) | The `space` subcommand. | |
| `src/verbs.rs` (Modify) | `Space(SpaceCommand)`, `--space` on `List`. `Move` calls `RestartIn`. | |
| `src/verbs/spaces.rs` (Create) | `muster space add\|rename\|rm NAME`, and `muster list spaces` listing each space's members. | |
| `src/verbs/list.rs` (Modify) | `--space NAME` filters through `rows_in`. Free terminals are listed under "Terminals". | |
| `src/verbs/terminals.rs` (Modify) | `muster term new --free`, and `--space NAME`. | |
| `src/verbs/start.rs`, `src/verbs/workspaces.rs` (Modify) | `StartSession { place: Workspace(..), space: None }`. | |
| `src/verbs/control.rs` (Modify) | `move_terminal` sends `RestartIn`; the client-side steps are removed. | One copy of the restart (ARC-31). |
| `src/verbs/targets.rs`, `src/commands.rs`, `src/json.rs` (Modify) | `session.workspace()`. `--json` gains `place` and `spaces`. | |

#### Desktop link (`apps/desktop/src-tauri`)

| File | Change | Why |
|---|---|---|
| `src/contract.rs` (Modify) | `Derived.space_rows: BTreeMap<SpaceId, Vec<Row>>`. `Patch` carries spaces upserted and removed, and `Gone.spaces`. | The webview never filters (ARC-24). |
| `src/link/patches.rs` (Modify) | Fill `space_rows` from `rows_in` for each space, and carry the space deltas. | |
| `src/link/patches_tests.rs`, `src/link/actor_tests.rs` (Modify) | The space lists after each delta. | |
| `src/commands.rs` (Modify) | Pass the new commands through, if this file lists commands by name. | |

#### Desktop webview (`apps/desktop/src`)

| File | Change | Why |
|---|---|---|
| `ipc/generated/*` (Regenerate) | `Session`, `SessionPlace`, `Space`, `Member`, `Command`, `Reply`, `Delta`, `Snapshot`, `Derived`, `Row`, `Gone` (`pnpm bindings`). | Types come from Rust. |
| `state/daemon.ts` (Modify) | Keep `spaces`, apply the patch's spaces. | |
| `state/ui.ts` (Modify) | `space: string \| null` for each window, kept in local storage inside try/catch, and falling back to All when the space is gone. | |
| `state/selectors.ts`, `state/selectors.test.ts` (Modify) | `workspaceOf(session)` reads `place`. `rowsShown(state, ui)` picks `rows` or `space_rows[space]`, with no filtering. | |
| `app/Sidebar.tsx`, `app/Sidebar.test.tsx` (Modify) | The space switcher at the top, from `src/ui`'s menu, offering New, Rename and Delete, with "New space" alone until a space exists. | |
| `screens/sessions/tree.ts`, `tree.test.ts`, `useTree.ts`, `TreeRows.tsx`, `SessionTree.tsx` (Modify) | Draw the `Terminals` row and free terminals. `session.workspace` becomes `place`. | |
| `session/useSessionMenu.ts`, `session/useSessionActions.tsx` (Modify) | "Add to space…", "Remove from <space>", "File under <project>", "Add <folder> as a project", and "Start a new terminal in <project>…", offered from `ReadPlace`. | |
| `flows/SpaceDialogs.tsx` (Create) | New, rename and delete space. Delete says the projects and terminals stay. | |
| `flows/RestartIn.tsx` (Create) | The restart dialog, naming the program that ends. | |
| `flows/Flows.tsx`, `flows/catalogue.ts`, `flows/Palette.tsx`, `flows/start.ts`, `flows/AddProject.tsx`, `flows/CloneProject.tsx` (Modify) | Register the new dialogs and palette entries, start a free terminal, and send `space` on start and add. | |
| `session/SessionPane.tsx`, `session/byHand.ts`, `session/changes/ChangesView.tsx`, `ClashBanners.tsx`, `PutBack.test.tsx`, `ChangesView.test.tsx` (Modify) | `place`. A free session shows "no project: no undo, no changes view" where the changes would be. | |
| `panels/beside.ts`, `panels/PaneActions.tsx`, `panels/usePrefixKeys.ts` (Modify) | `place`. No panel action beside a free terminal. | |
| `screens/sessions/AllSessions.tsx`, `ProjectPage.tsx`, `projectOrder.ts` (Modify) | `place`. | |
| `screens/queue/QueueScreen.tsx`, `QueueEntry.tsx`, `queue.ts` (Modify) | `place`, and each entry's space note (G6). | |
| `screens/tickets/Cleanup.tsx`, `TicketRow.tsx`, `Detail.tsx`, `agents.ts`, `flows/Questions.tsx` (Modify) | `place`. | |
| `ipc/mock/commands.ts`, `model.ts`, `sessions.ts`, `flows.ts`, `tickets.ts`, `tickets.test.ts`, `settings.ts`, `byHand.ts`, `scenario.ts` (Modify) | The mock daemon speaks spaces and free terminals, and its sessions have `place`. | `pnpm dev:mock` and the vitest tests run against it. |
| `keys/keymap.ts` (Modify) | Next and previous space from the shared key table. | |
| `e2e/sessions.spec.ts` (Modify) | Spaces and a free terminal in the mock window (CUJ 1, 2, 6). | |
| `e2e-app/sessions.spec.ts` (Modify) | The same against the real app and daemon, plus filing in place (CUJ 3). | Run before a release only. |

#### Documents

| File | Change | Why |
|---|---|---|
| `PRODUCT.md` (Modify) | §2: spaces and free terminals in the product's words. §6, §6a and §6b: the switcher, the Terminals section, the session menu items, and `muster space`. | The rules the code follows. |
| `ARCHITECTURE.md` (Modify) | §2: `SessionPlace` and `Space`. §4: store V11. §5: putting a terminal under a project and never typing into a shell. §5a, §5c: `rows_in` and `space_rows`. §6: protocol 57 and the raised minimum. §14: a new rule, **ARC-33**: "A session's place is typed: in a workspace or free. A rule that needs a workspace matches both cases. Muster never writes to a person's shell to move it." | Code and rules must not drift. |
| `apps/desktop/design/system/README.md` (Modify) | Only if the switcher needs a new component; otherwise unchanged. | ARC-26 table of components. |
| `docs/plugins.md` (Modify) | A free terminal's events carry no project or workspace, and no panel opens beside one. | Plugin authors' contract. |
| `concepts_brief.md`, `design_draft.md` (Delete, after this document is saved under `docs/designs/`) | Superseded. | |

#### End-to-end tests (`crates/muster/tests`)

| File | Change | Why |
|---|---|---|
| `spaces.rs` (Create) | Against the real daemon:<br>• create, rename and delete spaces, with the name rules;<br>• add and remove members, and two clients see the same;<br>• removing a project removes its memberships;<br>• `space` on start and add;<br>• a restart in place keeps spaces. | G1, G3, G7. |
| `free_terminals.rs` (Create) | A free terminal:<br>• starts in a stand-in home;<br>• an agent with no place is refused;<br>• `ReadPlace` reads its folder after a scripted `cd` in the test's own shell;<br>• `FileSession` files it in place, and is refused when the folder isn't inside the workspace, or has changed;<br>• `AddProjectHere` works, and is refused for the home folder;<br>• `RestartIn` names the program, and is refused while an agent runs;<br>• it survives `muster reload` running, and ends after a daemon stop. | G4, G5. |
| `cli_workspaces.rs`, `shells.rs`, `terminal_agents.rs` (Modify) | `muster move` through `RestartIn`. A `muster exec` agent in a free terminal is heard. | |
| `upgrade.rs`, `lifecycle.rs` (Modify) | A 56 client is told to update; a 57 client against a 56 daemon gets `DaemonTooOld`. | The raised minimum. |
| `tui_spaces.rs` (Create) | The TUI against the real daemon: switch spaces, the list follows, and a free terminal's menu. | |
| `support/*` (Modify) | A helper to start a free terminal and set a stand-in `HOME`. | |
| Other files that build sessions with `workspace` | Use `SessionPlace::Workspace`. | The compiler finds them. |

### Verification Plan

*   **Compilation and build:** `cargo check -p <crate>` per crate while working. `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets`. `cargo fmt --check`.
*   **Unit tests while working:** `cargo nextest run -p <crate> <filter>` for the test being written.
*   **Before finishing each crate:** `-p muster-core`, `-p muster-daemon -p muster-state`, `-p muster-model -p muster-tui`, `-p muster-client`, `-p muster-notch`.
*   **Whole suite:** `cargo nextest run --workspace` once, because the protocol version changes (AGENTS.md). The whole-suite hook asks the person first.
*   **Desktop:** `pnpm bindings`, `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm e2e` in `apps/desktop`. Before the release only: `pnpm e2e:app` on a Mac.
*   **CI's Linux-only steps, run locally before pushing:** `RUSTDOCFLAGS=-Dwarnings cargo doc`, `typos`, `cargo shear`, doc tests, `scripts/check-file-size.sh`.
*   **By hand, in `pnpm dev:mock` and the real app:**
    *   compare the sidebar pixel for pixel with no space and no free terminal against v0.7.0 (G7);
    *   walk CUJs 1–6 in both interfaces.
*   **On a real VM:** `muster ssh vm --install` to 57, then `--on vm` with a free terminal and a space.

--------------------------------------------------------------------------------

## As built (2026-10-25)

Where the build differs from the plan above, and what is not done yet.

*   **The place is `Option<WorkspaceId>`, not a `SessionPlace` enum.** `Session.workspace` and `StartSession.workspace` became optional: `None` is a free terminal. The compiler finds every place that must decide, as the enum would have, with less churn. `Session::is_free()` and `Session::is_in(workspace)` serve the common questions.
*   **Keys.** The spaces menu is `W` (both `S` and `G` clashed with keys people or plugins already set), a free terminal `+`, putting one under a project `F`, next and previous space `]` and `[`.
*   **Protocol.** Version 57 is also the oldest spoken, so `ClientFrame::since()` returns it for every frame; the check stays for later versions.
*   **Desktop.** The projects' heading reads *Projects* until a space exists, then names the space shown and opens the menu; with no space the window looks as before. The window's space is kept for the window (`muster.space`).
*   **Restart elsewhere.** The new terminal starts at the default size; the clients showing it give it theirs.
*   **Not done yet:**
    *   the space shown on each *Needs you* item, and the "in Work, Home" note on a line (G6's note);
    *   a "no project: no undo, no changes view" note in the desktop pane of a free terminal;
    *   a project cloned from GitHub joining the space shown;
    *   end-to-end tests of the TUI over the real daemon (`tui_spaces.rs`) and of the real app (`e2e-app`);
    *   the whole test suite, `pnpm e2e:app` and a try on a real VM.

--------------------------------------------------------------------------------

*Drafted with EGM*
