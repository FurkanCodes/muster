// The queue's list. Its order is the model's, never the webview's
// (`ARC-24`): *Needs you* is `derived.needs_you`, what asks first, then
// news not seen, oldest first. The agents at work are not in it: they need
// nobody yet, and *All* lists them.

import type { Session } from "../../ipc/generated/Session";
import type { SessionId } from "../../ipc/generated/SessionId";
import type { DaemonState } from "../../state/daemon";
import { standsBeside } from "../../state/selectors";

/** The section a session of the queue is shown in. */
export type Section = "asking" | "news" | "working";

export function sectionOf(session: Pick<Session, "status" | "unseen">): Section {
  if (session.status.state === "needs_you") return "asking";
  if (session.status.state === "running" && !session.unseen) return "working";
  return "news";
}

/** The sessions *Needs you* lists, in the model's order: what asks, then what finished unseen. */
export function queueIds(state: DaemonState): SessionId[] {
  return state.derived.needs_you.filter((id) => {
    const session = state.sessions[id];
    return Boolean(session && !standsBeside(session));
  });
}

/**
 * The list as shown, and its selection. The selected session keeps its
 * place when it leaves the list while it `stays`, as one the person has just
 * opened (opening it cleared its news) or answered (it works again), so the
 * list does not move under them; it goes once they select another. Any
 * other that leaves gives its place to the one after it.
 */
export function holdPlace(
  list: readonly SessionId[],
  shown: readonly SessionId[],
  selected: SessionId | undefined,
  stays: boolean,
): { ids: readonly SessionId[]; selected: SessionId | undefined } {
  if (selected && list.includes(selected)) return { ids: list, selected };
  const at = selected ? shown.indexOf(selected) : -1;
  if (selected && stays && at >= 0) {
    const ids = [...list];
    ids.splice(Math.min(at, ids.length), 0, selected);
    return { ids, selected };
  }
  return { ids: list, selected: list[Math.min(Math.max(at, 0), list.length - 1)] };
}

/** The project `session` is in, by its workspace; empty where none is known. */
export function projectOfSession(state: DaemonState, session: SessionId): string {
  return state.workspaces[state.sessions[session]?.workspace ?? ""]?.project ?? "";
}

/** A project's part of the queue: its sessions, in the queue's order. */
export interface ProjectGroup {
  project: string;
  ids: readonly SessionId[];
}

/**
 * The queue by project, as the All list groups sessions: each project once,
 * in the order its first session comes, so the project with the most
 * urgent comes first, and under it its sessions in the queue's order.
 */
export function byProject(
  ids: readonly SessionId[],
  projectOf: (id: SessionId) => string,
): ProjectGroup[] {
  const groups = new Map<string, SessionId[]>();
  for (const id of ids) {
    const project = projectOf(id);
    const group = groups.get(project);
    if (group) group.push(id);
    else groups.set(project, [id]);
  }
  return [...groups].map(([project, grouped]) => ({ project, ids: grouped }));
}
