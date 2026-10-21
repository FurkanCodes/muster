// The session pane's keys for what stands beside it, as the person set them
// (the keymap, `src/keys`): `shell` the shell, `full_screen` full screen,
// each plugin's panel on its own key (`plugin.<id>.<panel>`), and Esc out of
// full screen. While a terminal has the keys every key is its program's, so
// these act only while Muster has them.

import { useEffect } from "react";
import { type App, useApp, useDaemon, useUi } from "../app/context";
import { useKeys } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import { KEY_SCREENS, useActionKeys, useKeymap } from "../keys/useKeymap";
import { giveKeys, running, togglePanel, toggleShell } from "./beside";

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

export function usePanelKeys(session: SessionId, plugin: SessionId | null): void {
  const app = useApp();
  const screen = useUi((state) => KEY_SCREENS[state.screen]);
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const fullScreen = useUi((state) => state.fullScreen === session);
  const panels = usePanelHandlers({ session, workspace });

  useActionKeys(screen, {
    ...panels,
    full_screen: () => toggleFullScreen(app, session),
    shell: () => {
      if (workspace) void toggleShell(app, workspace, session);
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

  // A shell beside the session that ended goes, and the keys with it.
  const keptShell = useUi((state) => state.shell);
  const shellEnded = useDaemon((state) => {
    const stored = keptShell ? state.sessions[keptShell] : undefined;
    return Boolean(stored && !running(stored));
  });
  useEffect(() => {
    if (!shellEnded || !keptShell) return;
    const ui = app.ui.getState();
    if (ui.keysIn === "terminal" && ui.terminal === keptShell) {
      app.actions.showTerminal(session);
      app.actions.keysTo("list");
    }
    app.actions.showShell(null);
  }, [app, shellEnded, keptShell, session]);

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
