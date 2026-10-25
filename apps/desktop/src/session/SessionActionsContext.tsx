// The shown session's actions, held once for the window: what its list's
// keys and its pane's menu do to it (rename it in its line, close it after
// asking). The list and the pane beside it read the same actions, so a
// rename begun from the pane's menu is the one its line shows, and the
// dialog that asks before closing is drawn once (composition: state lifted
// into a provider both read).

import { createContext, type ReactNode, use } from "react";
import { useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { CloseSession, type SessionActions, useSessionActions } from "./useSessionActions";

const Actions = createContext<SessionActions | null>(null);

/** The session the shown list has selected, where its line is a session's: on *Needs you*, the queue's. */
export function useShownSession(): SessionId | undefined {
  const id = useUi((state) =>
    state.screen === "needs-you" ? state.selected["needs-you"] : state.selected[state.list],
  );
  return useDaemon((state) => (id && state.sessions[id] ? id : undefined));
}

export function SessionActionsProvider({ children }: { children: ReactNode }) {
  const actions = useSessionActions(useShownSession());
  return (
    <Actions value={actions}>
      {children}
      <CloseSession actions={actions} />
    </Actions>
  );
}

export function useShownSessionActions(): SessionActions {
  const actions = use(Actions);
  if (!actions) throw new Error("useShownSessionActions() outside <SessionActionsProvider>");
  return actions;
}
