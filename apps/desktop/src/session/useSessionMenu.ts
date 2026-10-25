// What the shown session's ⋯ menu (and a right-click on its line) offers,
// whichever list it was chosen in: pinning it to the right in a panel of its
// own (or taking it off the screen), renaming a chat, its checks, a review,
// a pull request and the way to it, and closing it, each with its key.

import { useApp, useDaemon } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { useKeymap } from "../keys/useKeymap";
import { keepLayout, keptLayout, placedOrTaken } from "../panels/beside";
import { Layout } from "../panels/layout";
import { standsBeside } from "../state/selectors";
import { keysOf, type MenuItem } from "../ui/Menu";
import * as byHand from "./byHand";
import { useShownSessionActions } from "./SessionActionsContext";

export function useSessionMenu(session: SessionId | undefined): MenuItem[] {
  const app = useApp();
  const map = useKeymap();
  const tools = useShownSessionActions();
  const isChat = useDaemon((state) => {
    const stored = session ? state.sessions[session] : undefined;
    return Boolean(stored && !standsBeside(stored));
  });
  const hasPullRequest = useDaemon((state) => {
    const workspace = session ? state.sessions[session]?.workspace : undefined;
    return Boolean(workspace && state.workspaces[workspace]?.pull_request);
  });
  const placed = useDaemon((state) =>
    Boolean(
      session &&
        Layout.read(state.preferences?.panels ?? null)
          .placed()
          .includes(session),
    ),
  );
  const pin = () => {
    if (!session) return;
    const next = placedOrTaken(keptLayout(app), session);
    void keepLayout(app, next.layout);
    app.actions.tell(next.said);
  };
  return [
    {
      ...keysOf(map, "pin_session"),
      label: placed ? "Take it off the screen" : "Pin to the right",
      does: pin,
    },
    ...(isChat ? [{ ...keysOf(map, "rename"), label: "Rename", does: tools.rename }] : []),
    { ...keysOf(map, "checks"), label: "Run the checks", does: () => byHand.runChecks(app) },
    { ...keysOf(map, "review"), label: "Review the change", does: () => byHand.review(app) },
    {
      ...keysOf(map, "open_pull_request"),
      label: "Open a draft pull request",
      does: () => byHand.openPullRequest(app),
    },
    ...(hasPullRequest
      ? [
          {
            ...keysOf(map, "pull_request_in_browser"),
            label: "Pull request in the browser",
            does: () => byHand.browsePullRequest(app),
          },
        ]
      : []),
    { ...keysOf(map, "close"), label: "Close", does: () => tools.close(), danger: true },
  ];
}
