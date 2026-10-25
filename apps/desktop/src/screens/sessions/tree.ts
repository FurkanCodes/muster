// The All list as drawn in the sidebar: each project, and under it its
// sessions in the model's order (`derived.rows`: workspace by workspace,
// chats before what stands by them), but for what a folded project hides.
// A worktree's branch and what its work comes to are said over its
// session, so it has a line of its own only while no session is in it, to
// start one there or remove it. A shell or a plugin's panel stands by the
// chats of its workspace, opened from theirs, so it has a line only where
// its workspace has no chat. Which projects are folded is the window's own;
// the daemon is never told. Nothing here orders anything.

import type { Session } from "../../ipc/generated/Session";
import type { DaemonState } from "../../state/daemon";
import { standsBeside } from "../../state/selectors";

/** A line of the list, with the project it is under. */
export interface TreeRow {
  kind: "project" | "workspace" | "session" | "terminals";
  id: string;
  /** The project it is under; {@link TERMINALS} for the terminals in no project. */
  project: string;
  /** A session's workspace, a worktree's own id; null for a project, and for a terminal in none. */
  workspace: string | null;
}

/** The id of the heading over the terminals in no project (`ARC-33`), which folds as a project does. */
export const TERMINALS = "terminals";

/** The lines the window's space lists (`Derived::space_rows`), or all of them. */
export function listedRows(
  state: DaemonState,
  space: string | null,
): DaemonState["derived"]["rows"] {
  if (space === null || !state.spaces.some((one) => one.id === space)) return state.derived.rows;
  return state.derived.space_rows[space] ?? [];
}

/** Whether a shell or a plugin's panel has a chat in its workspace to stand by. */
function hasChat(state: DaemonState, beside: Session): boolean {
  if (beside.workspace === null) return false;
  return Object.values(state.sessions).some(
    (session) => session.workspace === beside.workspace && !standsBeside(session),
  );
}

function hasSession(state: DaemonState, workspace: string): boolean {
  return Object.values(state.sessions).some((session) => session.workspace === workspace);
}

/** The model's projects and sessions, and worktrees with none, each with what it is under. */
export function allRows(state: DaemonState, space: string | null = null): TreeRow[] {
  const out: TreeRow[] = [];
  let inProject = "";
  for (const row of listedRows(state, space)) {
    if (row.kind === "terminals") {
      inProject = TERMINALS;
      out.push({ kind: "terminals", id: TERMINALS, project: TERMINALS, workspace: null });
      continue;
    }
    if (row.kind === "project") {
      inProject = row.id;
      out.push({ kind: "project", id: row.id, project: row.id, workspace: null });
      continue;
    }
    if (row.kind === "workspace") {
      // A project's own checkout is its line; a worktree with no session, one of its own.
      const kind = state.workspaces[row.id]?.kind.kind;
      if (kind && kind !== "checkout" && !hasSession(state, row.id))
        out.push({ kind: "workspace", id: row.id, project: inProject, workspace: row.id });
      continue;
    }
    const session = state.sessions[row.id];
    if (!session || (standsBeside(session) && hasChat(state, session))) continue;
    out.push({ kind: "session", id: row.id, project: inProject, workspace: session.workspace });
  }
  return out;
}

/** The rows drawn: those a folded project hides left out. */
export function shownRows(rows: readonly TreeRow[], folded: ReadonlySet<string>): TreeRow[] {
  return rows.filter(
    (row) => row.kind === "project" || row.kind === "terminals" || !folded.has(row.project),
  );
}

/** The sessions under a project's line, as the model lists them, shells too; a session's own. */
export function sessionsOn(state: DaemonState, row: Pick<TreeRow, "kind" | "id">): Session[] {
  return Object.values(state.sessions).filter((session) => {
    if (row.kind === "session") return session.id === row.id;
    if (row.kind === "terminals") return session.workspace === null;
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

/** The folds after the person folds or unfolds `project`'s line (`App::fold`). */
export function toggled(folded: ReadonlySet<string>, project: string): Set<string> {
  const next = new Set(folded);
  if (!next.delete(project)) next.add(project);
  return next;
}

/** The line a fold of `project` takes the selection to, when it hides the selected one. */
export function selectionAfterFold(
  project: string,
  selected: TreeRow | undefined,
): string | undefined {
  return selected &&
    selected.kind !== "project" &&
    selected.kind !== "terminals" &&
    selected.project === project
    ? project
    : selected?.id;
}

/** The folds without the one that hides `row`, so it can be shown (`App::unfold_to`). */
export function unfoldedTo(folded: ReadonlySet<string>, row: TreeRow): ReadonlySet<string> {
  if (row.kind === "project" || row.kind === "terminals" || !folded.has(row.project)) return folded;
  const next = new Set(folded);
  next.delete(row.project);
  return next;
}

/** The folds that still mean something: a project's, while it is there. */
export function keptFolds(state: DaemonState, folded: ReadonlySet<string>): ReadonlySet<string> {
  const free = Object.values(state.sessions).some((session) => session.workspace === null);
  const kept = [...folded].filter(
    (id) => state.projects[id] !== undefined || (id === TERMINALS && free),
  );
  return kept.length === folded.size ? folded : new Set(kept);
}
