// The All list as drawn in the sidebar: each space's heading (`ARC-33`),
// under it its projects and its terminals in no project, and under each
// project its sessions in the model's order (`derived.rows`: workspace by
// workspace, chats before what stands by them), but for what a folded space
// or project hides.
// A worktree's branch and what its work comes to are said over its
// session, so it has a line of its own only while no session is in it, to
// start one there or remove it. A shell has a line of its own, under its
// workspace's chats; a plugin's panel stands by them, opened from theirs,
// so it has a line only where its workspace has no chat. Which projects are folded is the window's own;
// the daemon is never told. Nothing here orders anything.

import type { Member } from "../../ipc/generated/Member";
import type { Session } from "../../ipc/generated/Session";
import type { DaemonState } from "../../state/daemon";
import { shownHarness, standsBeside } from "../../state/selectors";

/** A line of the list, with the space and the project it is under. */
export interface TreeRow {
  kind: "space" | "project" | "workspace" | "session";
  id: string;
  /** The project it is under, a project's own id; empty for a space and a terminal in none. */
  project: string;
  /** The space it is under, a space's own id; empty for what is in none. */
  space: string;
  /** A session's workspace, a worktree's own id; null for a project, and for a terminal in none. */
  workspace: string | null;
  /** A workspace's line as its branch's label, and a session under one: where its project's sessions work in more than its checkout. */
  place?: true;
}

/** What a fold of `row`'s line folds: a space's or a project's own; else the project it is under, or its space. */
export function foldOf(row: Pick<TreeRow, "kind" | "id" | "project" | "space">): string {
  if (row.kind === "space" || row.kind === "project") return row.id;
  return row.project || row.space;
}

/** The lines the window's space lists (`Derived::space_rows`), or all of them. */
export function listedRows(
  state: DaemonState,
  space: string | null,
): DaemonState["derived"]["rows"] {
  if (space === null || !state.spaces.some((one) => one.id === space)) return state.derived.rows;
  return state.derived.space_rows[space] ?? [];
}

/** A plugin's panel: no chat of its own, and no line where there is one to stand by. */
function isPanel(session: Session): boolean {
  return standsBeside(session) && shownHarness(session) !== "shell";
}

/** Whether a shell or a plugin's panel has a chat in its workspace to stand by. */
function hasChat(state: DaemonState, beside: Session): boolean {
  if (beside.workspace === null) return false;
  return Object.values(state.sessions).some(
    (session) => session.workspace === beside.workspace && !standsBeside(session),
  );
}

/** The space holding `member`; empty where none does. */
function holding(state: DaemonState, member: Member): string {
  const key = JSON.stringify(member);
  return (
    state.spaces.find((space) => space.members.some((held) => JSON.stringify(held) === key))?.id ??
    ""
  );
}

function hasSession(state: DaemonState, workspace: string): boolean {
  return Object.values(state.sessions).some((session) => session.workspace === workspace);
}

/** The model's projects and sessions, and worktrees with none, each with what it is under. */
export function allRows(state: DaemonState, space: string | null = null): TreeRow[] {
  const out: TreeRow[] = [];
  let inSpace = "";
  let inProject = "";
  const labelled = labelledProjects(state);
  for (const row of listedRows(state, space)) {
    if (row.kind === "space") {
      inSpace = row.id;
      inProject = "";
      out.push({ kind: "space", id: row.id, project: "", space: row.id, workspace: null });
      continue;
    }
    if (row.kind === "project") {
      inProject = row.id;
      // As the daemon holds it, not as the heading before: what is in none comes last.
      inSpace = holding(state, { Project: row.id });
      out.push({ kind: "project", id: row.id, project: row.id, space: inSpace, workspace: null });
      continue;
    }
    if (row.kind === "workspace") {
      // Where its project's sessions work beyond its checkout, each place
      // with a session is labelled with its branch, its sessions under it;
      // a worktree with no session has a line of its own, to start one there.
      const kind = state.workspaces[row.id]?.kind.kind;
      const placed = labelled.has(inProject) && hasSession(state, row.id);
      if (placed)
        out.push({
          kind: "workspace",
          id: row.id,
          project: inProject,
          space: inSpace,
          workspace: row.id,
          place: true,
        });
      else if (kind && kind !== "checkout" && !hasSession(state, row.id))
        out.push({
          kind: "workspace",
          id: row.id,
          project: inProject,
          space: inSpace,
          workspace: row.id,
        });
      continue;
    }
    const session = state.sessions[row.id];
    // A plugin's panel stands by its workspace's chat, opened from it; a
    // shell has a line of its own, so one started anywhere can be opened.
    if (!session || (isPanel(session) && hasChat(state, session))) continue;
    // A terminal in no project is under its space alone.
    if (session.workspace === null) {
      inProject = "";
      inSpace = holding(state, { Session: session.id });
    }
    out.push({
      kind: "session",
      id: row.id,
      project: inProject,
      space: inSpace,
      workspace: session.workspace,
      ...(labelled.has(inProject) ? { place: true as const } : {}),
    });
  }
  return out;
}

/**
 * The projects whose sessions are told apart by their places' branches:
 * where any works beyond its checkout, in a worktree or a run's container.
 * Two agents on one branch are always in one place, so its label gathers
 * them. Worked out once for the list, in one pass over its sessions.
 */
function labelledProjects(state: DaemonState): Set<string> {
  const out = new Set<string>();
  for (const session of Object.values(state.sessions)) {
    if (session.workspace === null || (isPanel(session) && hasChat(state, session))) continue;
    const workspace = state.workspaces[session.workspace];
    if (workspace && workspace.kind.kind !== "checkout") out.add(workspace.project);
  }
  return out;
}

/** The rows drawn: those a folded space or project hides left out. */
export function shownRows(rows: readonly TreeRow[], folded: ReadonlySet<string>): TreeRow[] {
  return rows.filter((row) => {
    if (row.kind === "space") return true;
    if (row.space && folded.has(row.space)) return false;
    return row.kind === "project" || !row.project || !folded.has(row.project);
  });
}

/** The sessions under a project's line, as the model lists them, shells too; a session's own. */
export function sessionsOn(state: DaemonState, row: Pick<TreeRow, "kind" | "id">): Session[] {
  return Object.values(state.sessions).filter((session) => {
    if (row.kind === "session") return session.id === row.id;
    if (row.kind === "space") {
      const space = state.spaces.find((one) => one.id === row.id);
      const workspace = session.workspace ? state.workspaces[session.workspace] : undefined;
      return Boolean(
        space?.members.some((held) =>
          "Project" in held
            ? held.Project === workspace?.project
            : session.workspace === null && held.Session === session.id,
        ),
      );
    }
    if (session.workspace === null) return false;
    if (row.kind === "workspace") return session.workspace === row.id;
    return state.workspaces[session.workspace]?.project === row.id;
  });
}

/**
 * The session that says most of how a line stands (`Model::most_urgent`):
 * in the order *Needs you* goes, one asking the person, then one with news
 * not seen, oldest first; else one at work, as the list has them.
 */
export function mostUrgent(state: DaemonState, row: Pick<TreeRow, "kind" | "id">): Session | null {
  const on = sessionsOn(state, row);
  const ids = new Set(on.map((session) => session.id));
  const first = state.derived.needs_you.find((id) => ids.has(id));
  if (first) return state.sessions[first] ?? null;
  for (const listed of state.derived.rows) {
    const session =
      listed.kind === "session" && ids.has(listed.id) ? state.sessions[listed.id] : null;
    if (session?.status.state === "running") return session;
  }
  return null;
}

/** The folds after the person folds or unfolds a space's or a project's line `id` (`App::fold`). */
export function toggled(folded: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(folded);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** The line a fold of `id` takes the selection to, when it hides the selected one. */
export function selectionAfterFold(id: string, selected: TreeRow | undefined): string | undefined {
  if (!selected || selected.id === id) return selected?.id;
  const under =
    (selected.kind !== "space" && selected.space === id) ||
    (selected.kind !== "project" && selected.kind !== "space" && selected.project === id);
  return under ? id : selected.id;
}

/** The folds without those that hide `row`, so it can be shown (`App::unfold_to`). */
export function unfoldedTo(folded: ReadonlySet<string>, row: TreeRow): ReadonlySet<string> {
  const hiding = [
    row.kind !== "space" ? row.space : "",
    row.kind !== "space" && row.kind !== "project" ? row.project : "",
  ].filter((id) => id && folded.has(id));
  if (hiding.length === 0) return folded;
  const next = new Set(folded);
  for (const id of hiding) next.delete(id);
  return next;
}

/** The folds that still mean something: a project's or a space's, while it is there. */
export function keptFolds(state: DaemonState, folded: ReadonlySet<string>): ReadonlySet<string> {
  const kept = [...folded].filter(
    (id) => state.projects[id] !== undefined || state.spaces.some((space) => space.id === id),
  );
  return kept.length === folded.size ? folded : new Set(kept);
}
