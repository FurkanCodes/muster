// Renders the app, or part of it, against the mock daemon, for tests.

import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { App } from "../app/App";
import { AppContext, type App as AppValue } from "../app/context";
import { MockBridge } from "../ipc/mock/bridge";
import { workingDay } from "../ipc/mock/scenario";
import { createDaemon } from "../state/daemon";
import { createUi } from "../state/ui";

/** The whole window over a fresh working day. */
export function renderApp(bridge = new MockBridge(workingDay())) {
  return { bridge, ...render(<App bridge={bridge} />) };
}

/** A part of the window, with stores the test can drive directly. */
export function renderWithApp(node: ReactNode, bridge = new MockBridge(workingDay())) {
  const { store, actions } = createUi();
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  void bridge.open((message) => app.daemon.apply(message));
  return { app, bridge, ...render(<AppContext value={app}>{node}</AppContext>) };
}
