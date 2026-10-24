// A session's line carried from the sidebar to the pane, to be pinned there
// in a panel of its own (`PRODUCT.md` §6a). A drag over the pane cannot read
// what it carries until it is dropped, so the line says it here as it is
// taken up, and the pane reads it as the line passes over.

import type { SessionId } from "../ipc/generated/SessionId";

/** What a carried line is said to be, for the pane to take it. */
export const CARRIED = "application/x-muster-session";

let carried: SessionId | null = null;

/** A line taken up: `session`'s, carried until it is dropped or let go. */
export function carry(session: SessionId | null): void {
  carried = session;
}

/** The session whose line is being carried, if one is. */
export function carriedSession(): SessionId | null {
  return carried;
}
