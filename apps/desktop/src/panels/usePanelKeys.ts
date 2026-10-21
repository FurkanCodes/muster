// The session pane's keys for what stands beside it, the person's from
// Settings › Keys where they set them (`muster_core::keys`): `T` the shell
// (`shell`), `f` full screen (`full_screen`), each plugin's panel on its own
// key, and Esc out of full screen. While a terminal has the keys every key is
// its program's, so these act only while Muster has them.

import { useEffect } from "react";
import { type App, useApp, useDaemon, useUi } from "../app/context";
import { type KeyHandlers, useKeys } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import {
  chordOfWritten,
  giveKeys,
  type PanelAction,
  panelActions,
  running,
  togglePanel,
  toggleShell,
} from "./beside";

/** An action's keys as written: the person's, else Muster's own. */
export function writtenKeys(
  keys: Readonly<Record<string, string[] | undefined>> | undefined,
  action: string,
  defaults: readonly string[],
): readonly string[] {
  return keys?.[action] ?? defaults;
}

/** The chords of `written`, each bound to `does`. */
export function bind(
  handlers: Record<string, () => void>,
  written: readonly string[],
  does: () => void,
) {
  for (const key of written) handlers[chordOfWritten(key)] ??= does;
}

/** The plugins' panels with their keys. */
export function usePanelActions(): PanelAction[] {
  const plugins = useDaemon((state) => state.plugins);
  const keys = useDaemon((state) => state.preferences?.keys);
  return panelActions(plugins, keys ?? {});
}

/** Full screen on and off (`f`): the session fills the window with the keys, and gives it back. */
export function toggleFullScreen(app: App, session: SessionId): void {
  if (app.ui.getState().fullScreen === session) {
    app.actions.fillWindow(null);
    return;
  }
  app.actions.fillWindow(session);
  app.actions.showPane("terminal");
  giveKeys(app, session);
}

export function usePanelKeys(session: SessionId, plugin: SessionId | null): void {
  const app = useApp();
  const keys = useDaemon((state) => state.preferences?.keys);
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const fullScreen = useUi((state) => state.fullScreen === session);
  const actions = usePanelActions();

  const handlers: Record<string, () => void> = {};
  bind(handlers, writtenKeys(keys, "full_screen", ["f"]), () => toggleFullScreen(app, session));
  bind(handlers, writtenKeys(keys, "shell", ["T"]), () => {
    if (workspace) void toggleShell(app, workspace, session);
  });
  for (const action of actions) {
    bind(handlers, action.keys, () => void togglePanel(app, action, { session, workspace }));
  }
  if (fullScreen) handlers.Escape = () => app.actions.fillWindow(null);
  useKeys(handlers as KeyHandlers);

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
