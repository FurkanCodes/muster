// The session pane's keys for what stands beside it, as the person set them
// (the keymap, `src/keys`): `shell` a shell placed beside it, `full_screen`
// full screen, `pin_session` the session shown placed in a panel of its own
// (again: off the screen),
// each plugin's panel on its own key (`plugin.<id>.<panel>`), and Esc out of
// full screen. While a terminal has the keys every key is its program's, so
// these act only while Muster has them.

import { useEffect } from "react";
import { type App, useApp, useDaemon, useUi } from "../app/context";
import { useKeys } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import { KEY_SCREENS, useActionKeys, useKeymap } from "../keys/useKeymap";
import { giveKeys, openShell, placedOrTaken, running, togglePanel } from "./beside";
import type { PanelsLayout } from "./useLayout";

/** Full screen on and off: the session fills the window with the keys, and gives it back. */
export function toggleFullScreen(app: App, session: SessionId): void {
  if (app.ui.getState().fullScreen === session) {
    app.actions.fillWindow(null);
    return;
  }
  app.actions.fillWindow(session);
  app.actions.showPane("terminal");
  giveKeys(app, session);
}

/** What the plugins' panels' keys do for the line selected: each toggled there. */
export function usePanelHandlers(selected: {
  session: SessionId | null;
  workspace: string | null;
}): Record<string, () => void> {
  const app = useApp();
  const map = useKeymap();
  const handlers: Record<string, () => void> = {};
  for (const bound of map.panels) {
    handlers[bound.name] = () => void togglePanel(app, bound.action, selected);
  }
  return handlers;
}

export function usePanelKeys(
  session: SessionId,
  plugin: SessionId | null,
  arranged: PanelsLayout,
): void {
  const app = useApp();
  const screen = useUi((state) => KEY_SCREENS[state.screen]);
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const fullScreen = useUi((state) => state.fullScreen === session);
  const panels = usePanelHandlers({ session, workspace });

  useActionKeys(screen, {
    ...panels,
    full_screen: () => toggleFullScreen(app, session),
    shell: () => {
      if (workspace) void openShell(app, workspace, session);
    },
    pin_session: () => {
      const next = placedOrTaken(arranged.layout, session);
      arranged.keep(next.layout);
      app.actions.tell(next.said);
    },
  });
  useKeys(fullScreen ? { Escape: () => app.actions.fillWindow(null) } : {});

  // Full screen ends with the session shown: another selected, or the pane gone.
  useEffect(
    () => () => {
      if (app.ui.getState().fullScreen === session) app.actions.fillWindow(null);
    },
    [app, session],
  );

  // A shell with the keys that ended leaves its panel, and the keys come
  // back to the list.
  const typingInEnded = useDaemon((state) => {
    const ui = app.ui.getState();
    const stored =
      ui.keysIn === "terminal" && ui.terminal ? state.sessions[ui.terminal] : undefined;
    return Boolean(stored && stored.harness === "shell" && !running(stored));
  });
  useEffect(() => {
    if (!typingInEnded) return;
    app.actions.showTerminal(session);
    app.actions.keysTo("list");
  }, [app, typingInEnded, session]);

  // A plugin's panel with the keys that is no longer beside the session
  // (ended, or another's) gives them back (`App::keep_plugin_keys`).
  const keysIn = useUi((state) => state.keysIn);
  const terminal = useUi((state) => state.terminal);
  const typingInPanel = useDaemon((state) =>
    keysIn === "terminal" && terminal && terminal !== session
      ? Boolean(state.sessions[terminal]?.harness.startsWith("plugin:"))
      : false,
  );
  useEffect(() => {
    if (!typingInPanel || terminal === plugin) return;
    app.actions.showTerminal(session);
    app.actions.keysTo("list");
  }, [app, typingInPanel, terminal, plugin, session]);
}
