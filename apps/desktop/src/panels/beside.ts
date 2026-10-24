// What stands beside the agent (`PRODUCT.md` §6, `ARCHITECTURE.md` §5a): the
// workspace's shell on `T` and a plugin's panel on its key, as the terminal
// UI opens them (`app::shell`, `app::plugin_panel`). The shell is put beside
// the session and kept there until `T` puts it away, still running; a
// plugin's panel follows the selection: the newest running beside the
// session shown, else the newest on its workspace. Put away, a plugin's
// panel ends and leaves the list, as closing does.

import type { App } from "../app/context";
import type { Beside } from "../ipc/generated/Beside";
import type { Session } from "../ipc/generated/Session";
import type { SessionId } from "../ipc/generated/SessionId";
import type { TermSize } from "../ipc/generated/TermSize";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import type { PanelAction } from "../keys/keymap";
import { errorText } from "../lib/text";
import type { DaemonState } from "../state/daemon";
import { shownHarness } from "../state/selectors";

/** The size a session beside is started at, before its panel gives it its own. */
const STARTING_SIZE: TermSize = { cols: 80, rows: 24 };

const SHELL = "shell";
const PANEL = "plugin:";

export function running(session: Session | undefined): session is Session {
  return Boolean(session && session.ended_at === null && session.status.state !== "exited");
}

/** The shell beside `shown`: the one put there that still runs, and is not `shown` itself. */
export function shellBeside(
  state: DaemonState,
  shell: SessionId | null,
  shown: SessionId,
): SessionId | null {
  return shell && shell !== shown && running(state.sessions[shell]) ? shell : null;
}

/**
 * The plugin's panel beside `shown` (`App::plugin_panel`): the newest
 * running beside it, else the newest running on its workspace; never itself.
 */
export function panelBeside(state: DaemonState, shown: SessionId): SessionId | null {
  const workspace = state.sessions[shown]?.workspace;
  if (!workspace) return null;
  let besideIt: Session | null = null;
  let onWorkspace: Session | null = null;
  for (const session of Object.values(state.sessions)) {
    if (session.id === shown || session.workspace !== workspace) continue;
    if (!session.harness.startsWith(PANEL) || !running(session)) continue;
    if (session.beside === shown) {
      if (!besideIt || session.created_at > besideIt.created_at) besideIt = session;
    } else if (session.beside === null) {
      if (!onWorkspace || session.created_at > onWorkspace.created_at) onWorkspace = session;
    }
  }
  return (besideIt ?? onWorkspace)?.id ?? null;
}

function refused(app: App, error: unknown): void {
  app.actions.tell(errorText(error), true);
}

/** Gives a terminal beside the session the keys. */
export function giveKeys(app: App, session: SessionId): void {
  app.actions.showTerminal(session);
  app.actions.keysTo("terminal");
}

/** Takes the keys from a terminal beside `shown` that goes, back to the list. */
function keysBack(app: App, going: SessionId, shown: SessionId | null): void {
  const ui = app.ui.getState();
  if (ui.keysIn === "terminal" && ui.terminal === going) {
    app.actions.showTerminal(shown);
    app.actions.keysTo("list");
  }
}

/** The shell beside `shown` put away, running on. */
export function putShellAway(app: App, shown: SessionId | null): void {
  const shell = app.ui.getState().shell;
  if (shell) keysBack(app, shell, shown);
  app.actions.showShell(null);
  app.actions.tell("The shell is put away; it runs on.");
}

/** A plugin's panel put away: its program ended, and it leaves the list. */
export async function putPanelAway(
  app: App,
  panel: SessionId,
  title: string,
  shown: SessionId | null,
): Promise<void> {
  keysBack(app, panel, shown);
  app.actions.tell(`${title} is put away.`);
  try {
    await app.bridge.command({ StopSession: { session: panel } });
    removeWhenEnded(app, panel);
  } catch (error) {
    refused(app, error);
  }
}

/**
 * `T`: the workspace's shell beside the session shown, with the keys: the
 * first running there, or one started for it; again on that workspace, put
 * away, still running. Resolves to the shell shown, if one is.
 */
export async function toggleShell(
  app: App,
  workspace: WorkspaceId,
  shown: SessionId | null,
): Promise<SessionId | null> {
  const state = app.daemon.store.getState();
  const ui = app.ui.getState();
  const beside = shown ? shellBeside(state, ui.shell, shown) : null;
  if (beside && state.sessions[beside]?.workspace === workspace) {
    putShellAway(app, shown);
    return null;
  }
  const existing = state.derived.rows
    .map((row) => (row.kind === "session" ? state.sessions[row.id] : undefined))
    .find(
      (session) =>
        session?.workspace === workspace && shownHarness(session) === SHELL && running(session),
    );
  if (existing) {
    app.actions.showShell(existing.id);
    giveKeys(app, existing.id);
    return existing.id;
  }
  try {
    const reply = await app.bridge.command({
      StartSession: {
        workspace,
        harness: SHELL,
        title: null,
        size: STARTING_SIZE,
        prompt: null,
        started_by: null,
      },
    });
    if (typeof reply !== "object" || !("SessionStarted" in reply)) return null;
    const id = reply.SessionStarted.id;
    app.actions.showShell(id);
    giveKeys(app, id);
    return id;
  } catch (error) {
    refused(app, error);
    return null;
  }
}

/** Once `session` has ended, takes it off the list, as closing does. */
function removeWhenEnded(app: App, session: SessionId): void {
  const ended = () => !running(app.daemon.store.getState().sessions[session]);
  const remove = () => {
    if (!app.daemon.store.getState().sessions[session]) return;
    app.bridge.command({ RemoveSession: { session } }).catch((error) => refused(app, error));
  };
  if (ended()) {
    remove();
    return;
  }
  const stop = app.daemon.store.subscribe(() => {
    if (!ended()) return;
    stop();
    remove();
  });
}

/**
 * A panel's key (`App::toggle_panel`): the panel opened beside the session
 * selected, or on the selected line's workspace, as its manifest says; open
 * there already, put away, its program ended.
 */
export async function togglePanel(
  app: App,
  action: Pick<PanelAction, "plugin" | "panel" | "title">,
  selected: { session: SessionId | null; workspace: WorkspaceId | null },
): Promise<void> {
  const on = app.daemon.store
    .getState()
    .plugins.find((plugin) => plugin.id === action.plugin)
    ?.details?.panels.find((panel) => panel.id === action.panel)?.on;
  if (!on) return;
  let beside: Beside;
  if (on === "session") {
    if (!selected.session)
      return app.actions.tell(`Select a session to open ${action.title} beside it.`, true);
    beside = { session: selected.session };
  } else {
    if (!selected.workspace)
      return app.actions.tell(
        `Select a session, a worktree or a project to open ${action.title} on it.`,
        true,
      );
    beside = { workspace: selected.workspace };
  }
  const state = app.daemon.store.getState();
  const harness = `${PANEL}${action.plugin}/${action.panel}`;
  const open = Object.values(state.sessions).find(
    (session) =>
      session.harness === harness &&
      running(session) &&
      ("session" in beside
        ? session.beside === beside.session
        : session.workspace === beside.workspace && session.beside === null),
  );
  if (open) return putPanelAway(app, open.id, action.title, selected.session);
  try {
    await app.bridge.command({
      Plugin: {
        OpenPanel: { plugin: action.plugin, panel: action.panel, beside, size: STARTING_SIZE },
      },
    });
    app.actions.tell("The plugin's panel is open.");
  } catch (error) {
    refused(app, error);
  }
}
