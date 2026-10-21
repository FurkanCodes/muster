// What the queue remembers of what happened while the window was open: the
// session that came to need the person last (`l` goes to it, as in the
// terminal UI), and which sessions' status changed as the person watched, so
// their rows flash once. What a snapshot brings, at start or after the link
// came back, is no news: it neither flashes nor counts as needing them.

import type { Session } from "../../ipc/generated/Session";
import type { SessionId } from "../../ipc/generated/SessionId";
import type { Daemon, DaemonState } from "../../state/daemon";

interface History {
  /** The session that came to need the person last. */
  last: SessionId | undefined;
  /** Each session's status as it last changed in sight, as `stamp` says it. */
  changed: Map<SessionId, string>;
}

const histories = new WeakMap<Daemon, History>();

/** A session's status and since when, to tell one change from another. */
export function stamp(session: Pick<Session, "status" | "status_since">): string {
  return `${JSON.stringify(session.status)}@${session.status_since}`;
}

/** The session that came to need the person last, while it is still there. */
export function lastNeeded(daemon: Daemon): SessionId | undefined {
  const last = histories.get(daemon)?.last;
  return last && daemon.store.getState().sessions[last] ? last : undefined;
}

/** Whether `session` came to its status as the person watched, and so flashes. */
export function changedInSight(daemon: Daemon, session: Session): boolean {
  return histories.get(daemon)?.changed.get(session.id) === stamp(session);
}

function stamps(state: DaemonState): Map<SessionId, string> {
  const out = new Map<SessionId, string>();
  for (const session of Object.values(state.sessions)) out.set(session.id, stamp(session));
  return out;
}

/** Follows the daemon's state for the window's life; returns what stops it. */
export function installQueueHistory(daemon: Daemon): () => void {
  const history: History = { last: undefined, changed: new Map() };
  histories.set(daemon, history);
  let previous: Map<SessionId, string> | undefined;
  let needed = new Set<SessionId>();
  // The sessions held when the link went: until a snapshot replaces them, nothing is news.
  let held: DaemonState["sessions"] | null = null;
  const stop = daemon.store.subscribe((state) => {
    if (!state.loaded) return;
    if (state.link.state !== "up") {
      if (previous) held = state.sessions;
      previous = undefined;
      return;
    }
    if (!previous && held === state.sessions) return;
    const next = stamps(state);
    if (previous) {
      for (const [id, now] of next) {
        if (previous.get(id) !== now) history.changed.set(id, now);
      }
      // Of those that came to need the person in this change, the latest.
      let newest: Session | undefined;
      for (const id of state.derived.needs_you) {
        const session = state.sessions[id];
        if (!session || (needed.has(id) && previous.get(id) === next.get(id))) continue;
        if (!newest || session.status_since >= newest.status_since) newest = session;
      }
      if (newest) history.last = newest.id;
    }
    for (const id of history.changed.keys()) if (!next.has(id)) history.changed.delete(id);
    previous = next;
    needed = new Set(state.derived.needs_you);
    held = null;
  });
  return () => {
    stop();
    histories.delete(daemon);
  };
}
