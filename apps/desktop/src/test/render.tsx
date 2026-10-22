// Renders the app, or part of it, against the mock daemon, for tests.

import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { App } from "../app/App";
import { AppContext, type App as AppValue } from "../app/context";
import { Detail } from "../app/Detail";
import { LISTS } from "../app/screens";
import { MockBridge } from "../ipc/mock/bridge";
import { workingDay } from "../ipc/mock/scenario";
import { SessionActionsProvider } from "../session/SessionActionsContext";
import { createDaemon } from "../state/daemon";
import { createUi, type ListId } from "../state/ui";

/** The whole window over a fresh working day. */
export function renderApp(bridge = new MockBridge(workingDay())) {
  return { bridge, ...render(<App bridge={bridge} />) };
}

/** A part of the window, with stores the test can drive directly. */
export function renderWithApp(node: ReactNode, bridge = new MockBridge(workingDay())) {
  const { store, actions } = createUi();
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  void bridge.open((message) => app.daemon.apply(message));
  return {
    app,
    bridge,
    ...render(
      <AppContext value={app}>
        <SessionActionsProvider>{node}</SessionActionsProvider>
      </AppContext>,
    ),
  };
}

/**
 * A list as the window draws it: in its sidebar, and beside it the pane of
 * the line it has selected (app/Detail.tsx), the list shown first.
 */
export function renderList(list: ListId, active = true, bridge = new MockBridge(workingDay())) {
  const List = LISTS[list];
  const { store, actions } = createUi();
  actions.show(list);
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  void bridge.open((message) => app.daemon.apply(message));
  return {
    app,
    bridge,
    ...render(
      <AppContext value={app}>
        <SessionActionsProvider>
          <List active={active} />
          {/* Not active: a page is shown in the pane's place (app/App.tsx: Body). */}
          {active ? <Detail /> : null}
        </SessionActionsProvider>
      </AppContext>,
    ),
  };
}
