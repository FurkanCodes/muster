// The All sessions tree as drawn (`app::fold` in the terminal UI): the
// model's rows as they come (`derived.rows`: each project, its workspaces,
// their sessions, chats before the shells and panels that stand by them),
// narrowed to a project where the window is, but for what a folded line
// hides. Which lines are folded is the window's own; the daemon is never
// told. Nothing here orders anything.

import type { Row } from "../../ipc/generated/desktop/Row";
import type { Session } from "../../ipc/generated/Session";
import type { DaemonState } from "../../state/daemon";
import { standsBeside } from "../../state/selectors";

/** A line of the tree, with what it is under. */
export interface TreeRow {
  kind: Row["kind"];
  id: string;
  project: string;
  /** Its workspace: a workspace's own id, a session's workspace; null for a project. */
  workspace: string | null;
}

/** The model's rows with what each is under, narrowed to `project` where it is set. */
export function allRows(state: DaemonState, project: string | null): TreeRow[] {
  const out: TreeRow[] = [];
  let inProject = "";
  let inWorkspace: string | null = null;
  for (const row of state.derived.rows) {
    if (row.kind === "project") {
      inProject = row.id;
      inWorkspace = null;
    } else if (row.kind === "workspace") inWorkspace = row.id;
    if (project && inProject !== project) continue;
    out.push({
      kind: row.kind,
      id: row.id,
      project: inProject,
      workspace: row.kind === "project" ? null : inWorkspace,
    });
  }
  return out;
}

/** The rows drawn: those a folded project or workspace hides left out. */
export function shownRows(rows: readonly TreeRow[], folded: ReadonlySet<string>): TreeRow[] {
  return rows.filter((row) => {
    if (row.kind === "project") return true;
    if (folded.has(row.project)) return false;
    return row.kind === "workspace" || !folded.has(row.workspace ?? "");
  });
}

/** The sessions on a project's or a workspace's line, as the model lists them, shells too. */
export function sessionsOn(state: DaemonState, row: Pick<TreeRow, "kind" | "id">): Session[] {
  return Object.values(state.sessions).filter((session) => {
    if (row.kind === "session") return session.id === row.id;
    if (row.kind === "workspace") return session.workspace === row.id;
    return state.workspaces[session.workspace]?.project === row.id;
  });
}

/** Whether a line has anything to fold away: a project always, a workspace once a session is in it. */
export function foldable(state: DaemonState, row: Pick<TreeRow, "kind" | "id">): boolean {
  if (row.kind === "project") return true;
  if (row.kind === "session") return false;
  return Object.values(state.sessions).some((session) => session.workspace === row.id);
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

/**
 * Whether a shell (or a plugin's panel) is shown as its workspace's one
 * chat's (`Model::under_its_chat`): set in under it where the workspace has
 * one chat, after the chats it serves where it has several.
 */
export function underItsChat(state: DaemonState, shell: Session): boolean {
  if (!standsBeside(shell)) return false;
  let chats = 0;
  for (const session of Object.values(state.sessions)) {
    if (session.workspace === shell.workspace && !standsBeside(session)) chats += 1;
  }
  return chats === 1;
}

/**
 * The folds, after the person folds or unfolds `row` (`App::fold`): a line
 * with nothing under it does not fold, and says so.
 */
export function toggled(
  folded: ReadonlySet<string>,
  row: TreeRow,
  canFold: boolean,
): { folded: Set<string>; refused: boolean } {
  const next = new Set(folded);
  if (next.delete(row.id)) return { folded: next, refused: false };
  if (!canFold) return { folded: next, refused: true };
  next.add(row.id);
  return { folded: next, refused: false };
}

/** The line a fold of `folding` takes the selection to, when it hides the selected one. */
export function selectionAfterFold(
  folding: TreeRow,
  selected: TreeRow | undefined,
): string | undefined {
  if (!selected || selected.id === folding.id) return selected?.id;
  const under =
    folding.kind === "project"
      ? selected.project === folding.id
      : folding.kind === "workspace" && selected.kind === "session"
        ? selected.workspace === folding.id
        : false;
  return under ? folding.id : selected.id;
}

/** The folds without those that hide `row`, so it can be shown (`App::unfold_to`). */
export function unfoldedTo(folded: ReadonlySet<string>, row: TreeRow): ReadonlySet<string> {
  const hiding = [
    row.kind !== "project" ? row.project : null,
    row.kind === "session" ? row.workspace : null,
  ].filter((id): id is string => id !== null && folded.has(id));
  if (hiding.length === 0) return folded;
  const next = new Set(folded);
  for (const id of hiding) next.delete(id);
  return next;
}

/** The folds that still mean something: a project gone, or a workspace with no session left, unfolds. */
export function keptFolds(state: DaemonState, folded: ReadonlySet<string>): ReadonlySet<string> {
  const kept = [...folded].filter((id) =>
    state.projects[id]
      ? true
      : state.workspaces[id]
        ? foldable(state, { kind: "workspace", id })
        : false,
  );
  return kept.length === folded.size ? folded : new Set(kept);
}
