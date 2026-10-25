// What stands beside the agent (`PRODUCT.md` §6, `ARCHITECTURE.md` §5a): the
// workspace's shells, placed in panels of their own as any terminal is, and a
// plugin's panel on its key, as the terminal UI opens them (`app::shell`,
// `app::plugin_panel`). `T` places the workspace's shell beside the session
// shown: the keys go to one placed already, else the newest running there
// off the screen is placed, else a new one is started and placed; a shell
// that ends leaves its panel, the daemon taking it out of the layout. A
// plugin's panel follows the selection: the newest running beside the
// session shown, else the newest on its workspace. Put away, a plugin's
// panel ends and leaves the list, as closing does.

import type { App } from "../app/context";
import { replyAs } from "../ipc/ask";
import type { Beside } from "../ipc/generated/Beside";
import type { Session } from "../ipc/generated/Session";
import type { SessionId } from "../ipc/generated/SessionId";
import type { TermSize } from "../ipc/generated/TermSize";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import type { PanelAction } from "../keys/keymap";
import { errorText } from "../lib/text";
import type { DaemonState } from "../state/daemon";
import { shownHarness } from "../state/selectors";
import { Layout, type Place } from "./layout";

/** The size a session beside is started at, before its panel gives it its own. */
const STARTING_SIZE: TermSize = { cols: 80, rows: 24 };

const SHELL = "shell";
const PANEL = "plugin:";

export function running(session: Session | undefined): session is Session {
  return Boolean(session && session.ended_at === null && session.status.state !== "exited");
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
 * `session` placed in a panel of its own at the right of the preview, or,
 * placed already, taken off the screen, running on: what `pin_session` and
 * a line's menu do (`app::placed::toggle_place`); and what is said of it.
 */
export function placedOrTaken(
  layout: Layout,
  session: SessionId,
): { layout: Layout; said: string } {
  return layout.placed().includes(session)
    ? { layout: layout.unplaced(session), said: "Took it off the screen; it runs on." }
    : {
        layout: layout.placing(session, { beside: "session", edge: "right" }),
        said: "Placed it in a panel of its own.",
      };
}

/** The layout as the person's settings have it now. */
export function keptLayout(app: App): Layout {
  return Layout.read(app.daemon.store.getState().preferences?.panels ?? null);
}

/** Has the daemon keep `layout` in the person's settings; a refusal is said. */
export async function keepLayout(app: App, layout: Layout): Promise<void> {
  try {
    await app.bridge.command({ ChangePreference: { Panels: layout.toString() } });
  } catch (error) {
    refused(app, error);
  }
}

/** Whether `session` is a shell that still runs. */
function runningShell(session: Session | undefined): session is Session {
  return running(session) && shownHarness(session) === SHELL;
}

/**
 * `T`: the workspace's shell placed beside the session shown (at the
 * preview's right; along the window's right with none shown), with the keys:
 * the newest placed already, else the newest running there off the screen,
 * else a new one. Resolves to the shell, if one is.
 */
export async function openShell(
  app: App,
  workspace: WorkspaceId,
  shown: SessionId | null,
): Promise<SessionId | null> {
  const state = app.daemon.store.getState();
  const layout = keptLayout(app);
  const shells = Object.values(state.sessions)
    .filter((session) => session.workspace === workspace && runningShell(session))
    .sort((one, other) => one.created_at - other.created_at);
  const placed = new Set(layout.placed());
  const on = shells.filter((shell) => placed.has(shell.id)).at(-1);
  if (on) {
    giveKeys(app, on.id);
    return on.id;
  }
  const place: Place = shown ? { beside: "session", edge: "right" } : { window: "right" };
  const off = shells.at(-1);
  if (off) {
    await keepLayout(app, layout.placing(off.id, place));
    giveKeys(app, off.id);
    return off.id;
  }
  return startShell(app, workspace, place);
}

/**
 * A new shell in `workspace`, placed at `place` with the keys, as a panel
 * split by the panel key opens one. Resolves to it, once started.
 */
export async function startShell(
  app: App,
  workspace: WorkspaceId,
  place: Place,
): Promise<SessionId | null> {
  try {
    const reply = await app.bridge.command({
      StartSession: {
        workspace,
        harness: SHELL,
        title: null,
        size: STARTING_SIZE,
        space: null,
        prompt: null,
        started_by: null,
      },
    });
    const id = replyAs(reply, "SessionStarted")?.id;
    if (id === undefined) return null;
    await keepLayout(app, keptLayout(app).placing(id, place));
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
