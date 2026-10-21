// The queue's list. Its order is the model's, never the webview's
// (`ARC-24`): *Needs you* is `derived.needs_you` (asking first, then news
// not seen, oldest first) with the agents at work after it, the longest at
// it first (`derived.working`).

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

/** The sessions *Needs you* lists, in the model's order. */
export function queueIds(state: DaemonState): SessionId[] {
  const shown = (id: SessionId, keep: (session: Session) => boolean) => {
    const session = state.sessions[id];
    return Boolean(session && !standsBeside(session) && keep(session));
  };
  const waiting = state.derived.needs_you.filter((id) => shown(id, () => true));
  const working = state.derived.working.filter((id) =>
    shown(id, (session) => session.status.state === "running" && !session.unseen),
  );
  return [...waiting, ...working.filter((id) => !waiting.includes(id))];
}

/**
 * The list as shown, and its selection. The selected session keeps its
 * place when it leaves the list while it is not at work again, as one the
 * person has just opened (opening it cleared its news), so the list does not
 * move under them; it goes once they select another. Any other that leaves
 * gives its place to the one after it.
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
