// Everything the command palette goes to or does (design system:
// CommandPalette): actions on the selected thing first, as the TUI's
// right-click menu lists them (crates/muster-tui/src/app/actions.rs), then
// what can be done anywhere, the sessions (those asking first), the tickets
// and the settings. Each action runs as its key would.

import type { App } from "../app/context";
import { pressKey } from "../app/keys";
import type { Issue } from "../ipc/generated/Issue";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { Session } from "../ipc/generated/Session";
import * as byHand from "../session/byHand";
import { projectsInOrder } from "../state/selectors";
import type { ScreenId } from "../state/ui";
import * as act from "./actions";
import { type KeyAction, keysOf, labelOf } from "./keymap";
import { narrowed } from "./match";

export type Lead =
  | { kind: "session"; session: Session }
  | { kind: "ticket"; key: string }
  | { kind: "none" };

export interface PaletteItem {
  id: string;
  group: string;
  label: string;
  /** The key that does it, as a key cap shows it. */
  keys: string | null;
  /** Said quietly at its end: a session's or ticket's project, a setting's page. */
  meta: string | null;
  lead: Lead;
  run: () => void;
}

export interface Ticket {
  project: ProjectId;
  projectName: string;
  issue: Issue;
}

/** The labels a hint's key cap may show, as the key listener names those keys. */
const CHORDS: Record<string, string> = {
  "⏎": "Enter",
  "⇥": "Tab",
  "⇧⇥": "⇧Tab",
  "⎋": "Escape",
  "⌫": "Backspace",
  "↑": "ArrowUp",
  "↓": "ArrowDown",
  "←": "ArrowLeft",
  "→": "ArrowRight",
  space: " ",
};

const SCREENS: readonly [ScreenId, string, string | null][] = [
  ["all", "Sessions", "⌘1"],
  ["needs-you", "Needs you", "⌘2"],
  ["tickets", "Tickets", "⌘3"],
  ["settings", "Settings", "⌘,"],
];

/** Settings' pages, then each setting by its row (src/screens/settings/model.ts) and its name there. */
const SETTINGS: readonly [row: string, label: string, page: string][] = [
  ["general", "General", "Settings"],
  ["notifications", "Notifications", "Settings"],
  ["agents", "Agents", "Settings"],
  ["naming", "Naming chats", "Settings"],
  ["plugins", "Plugins", "Settings"],
  ["keys", "Keys", "Settings"],
  ["general/theme", "Theme", "General"],
  ["general/session_lines", "Lines for each session", "General"],
  ["general/panels", "Put the panels back as at first, a reset", "General"],
  ["general/colours", "Colours, in the terminal UI", "General"],
  ["general/side_by_side", "Session beside the list from", "General"],
  ["notifications/notifications", "Notifications, on or off", "Notifications"],
  ["notifications/tell.question", "Questions, told and their sound", "Notifications"],
  ["notifications/tell.finish", "Finishes, told and their sound", "Notifications"],
  ["notifications/tell.error", "Errors, told and their sound", "Notifications"],
  ["notifications/notch", "Agents in the notch", "Notifications"],
  ["agents/read_again", "Read your agents again", "Agents"],
  ["naming/names_by_agent", "Chats named by an agent", "Naming chats"],
  ["naming/named_by", "Named by", "Naming chats"],
  ["naming/naming_model", "Its model, naming chats", "Naming chats"],
  ["plugins/add_plugin", "Add a plugin", "Plugins"],
  ["keys/find", "Find a key, to set it", "Keys"],
];

function actionsHere(app: App): PaletteItem[] {
  const prefs = app.daemon.store.getState().preferences;
  const { session, workspace, project, onProject } = act.here(app);
  const group = session
    ? `Actions on ${session.title}`
    : onProject && project
      ? `Actions on ${project.name}`
      : workspace
        ? `Actions on ⎇ ${workspace.kind.branch || project?.name || ""}`
        : "Actions";
  const items: PaletteItem[] = [];
  // Each runs as its key does on the screen shown, which may answer it
  // itself (the queue renames in place); else as the flows do.
  const add = (action: KeyAction, label: string, run: (app: App) => void) =>
    items.push({
      id: `do:${action}`,
      group,
      label,
      keys: labelOf(prefs, action),
      meta: null,
      lead: { kind: "none" },
      run: () => {
        const key = keysOf(prefs, action)[0];
        if (!key || !pressKey(key.chord)) run(app);
      },
    });
  if (session && act.runs(session)) {
    add("rename", "Rename the session", act.rename);
    add("close", "Close the session", act.close);
    add("stop_session", "Stop the session", act.stop);
    add("new_session", "Start a new session in its worktree", act.newSession);
  } else if (session) {
    add("rename", "Rename the session", act.rename);
    add("restart_session", "Restart the session", act.restart);
    add("close", "Remove the session from the list", act.close);
    add("new_session", "Start a new session in its worktree", act.newSession);
  }
  // A session in a worktree stands for it, as the list gives the worktree no line of its own.
  if (session && workspace?.kind.kind === "worktree") {
    if (!act.plain(workspace)) {
      add("tidy", "Tidy the worktree's commits, with an agent", act.tidy);
      add("move_branch", "Move the worktree's branch onto its base", act.moveBranch);
    }
    items.push({
      id: "do:remove_worktree",
      group,
      label: "Remove the worktree",
      keys: null,
      meta: null,
      lead: { kind: "none" },
      run: () => act.removeWorktree(app),
    });
  }
  if (!session && onProject && project) {
    add("new_worktree", "Make a new worktree for the project", act.newWorktree);
    add("new_session", "Start a new session in its checkout", act.newSession);
    add("close", "Stop tracking the project", act.close);
  } else if (!session && workspace) {
    add("new_session", "Start a new session in the worktree", act.newSession);
    if (workspace.kind.kind === "worktree") add("close", "Remove the worktree", act.close);
    if (!act.plain(workspace) && workspace.kind.kind !== "container") {
      add("tidy", "Tidy the commits, with an agent", act.tidy);
      add("move_branch", "Move the branch onto its base", act.moveBranch);
    }
    add("new_worktree", "Make a new worktree for the project", act.newWorktree);
  }
  // The stages by hand, on a worktree or a session in one (src/session/byHand.ts).
  if (workspace && !onProject && workspace.kind.kind !== "container") {
    add("checks", "Run the project's checks", byHand.runChecks);
    if (!act.plain(workspace)) {
      add("review", "Have Claude Code review the change", byHand.review);
      add(
        "open_pull_request",
        "Push the branch and open a draft pull request",
        byHand.openPullRequest,
      );
    }
  }
  if (workspace?.pull_request && !onProject)
    add(
      "pull_request_in_browser",
      "Open the pull request in the browser",
      byHand.browsePullRequest,
    );
  // The screen's own actions, as its status bar hints say them.
  for (const hint of app.ui.getState().hints) {
    if (items.some((item) => item.keys === hint.key)) continue;
    const chord = CHORDS[hint.key] ?? hint.key;
    items.push({
      id: `hint:${hint.key}`,
      group,
      label: hint.does.charAt(0).toUpperCase() + hint.does.slice(1),
      keys: hint.key,
      meta: null,
      lead: { kind: "none" },
      run: () => pressKey(chord),
    });
  }
  return items;
}

function anywhere(app: App, taken: readonly PaletteItem[]): PaletteItem[] {
  const prefs = app.daemon.store.getState().preferences;
  const group = "Anywhere";
  const items: PaletteItem[] = [
    {
      id: "do:add_project",
      group,
      label: "Add a project",
      keys: labelOf(prefs, "add_project"),
      meta: null,
      lead: { kind: "none" },
      run: () => act.addProject(app),
    },
    {
      id: "do:clone_project",
      group,
      label: "Clone a project from GitHub",
      keys: labelOf(prefs, "clone_project"),
      meta: null,
      lead: { kind: "none" },
      run: () => act.cloneProject(app),
    },
  ];
  if (!taken.some((item) => item.id === "do:new_session"))
    items.push({
      id: "do:new_session",
      group,
      label: "Start a new session",
      keys: labelOf(prefs, "new_session"),
      meta: null,
      lead: { kind: "none" },
      run: () => act.newSession(app),
    });
  for (const [screen, label, keys] of SCREENS)
    items.push({
      id: `screen:${screen}`,
      group,
      label: `Go to ${label}`,
      keys,
      meta: null,
      lead: { kind: "none" },
      run: () => app.actions.show(screen),
    });
  items.push(
    {
      id: "fold-sidebar",
      group,
      label: "Fold the sidebar away, or bring it back",
      keys: "⌘\\",
      meta: null,
      lead: { kind: "none" },
      run: () => app.actions.foldSidebar(),
    },
    {
      id: "help",
      group,
      label: "Every key of this screen",
      keys: labelOf(prefs, "help"),
      meta: null,
      lead: { kind: "none" },
      run: () => app.actions.openPalette(true, "keys"),
    },
  );
  return items;
}

function sessions(app: App): PaletteItem[] {
  const state = app.daemon.store.getState();
  const asking = new Set(state.derived.needs_you);
  const order = [
    ...state.derived.needs_you,
    ...state.derived.rows
      .filter((row) => row.kind === "session" && !asking.has(row.id))
      .map((row) => row.id),
  ];
  return order.flatMap((id) => {
    const session = state.sessions[id];
    if (!session) return [];
    const project = state.projects[state.workspaces[session.workspace]?.project ?? ""];
    return [
      {
        id: `session:${id}`,
        group: "Sessions",
        label: session.title,
        keys: null,
        meta: project?.name ?? null,
        lead: { kind: "session", session } as const,
        run: () => {
          const screen = asking.has(id) ? "needs-you" : "all";
          app.actions.show(screen);
          app.actions.select(screen, id);
          app.actions.showTerminal(id);
        },
      },
    ];
  });
}

function tickets(app: App, found: readonly Ticket[]): PaletteItem[] {
  return found.map(({ project, projectName, issue }) => ({
    id: `ticket:${project}:${issue.key}`,
    group: "Tickets",
    label: issue.title,
    keys: null,
    meta: projectName,
    lead: { kind: "ticket", key: /^\d+$/.test(issue.key) ? `#${issue.key}` : issue.key },
    run: () => {
      app.actions.show("tickets");
      app.actions.select("tickets", issue.key);
    },
  }));
}

/** Settings' pages; with something typed, each setting too, and each project's page. */
function settings(app: App, typed: string): PaletteItem[] {
  const go = (row: string) => () => {
    app.actions.show("settings");
    app.actions.select("settings", row);
  };
  const own = SETTINGS.filter(([row]) => typed.trim() !== "" || !row.includes("/")).map(
    ([row, label, page]) => ({
      id: `settings:${row}`,
      group: "Settings",
      label,
      keys: null,
      meta: page,
      lead: { kind: "none" } as const,
      run: go(row),
    }),
  );
  if (typed.trim() === "") return own;
  // Each project's page, under its name.
  const projects = projectsInOrder(app.daemon.store.getState()).map((project) => ({
    id: `settings:project:${project.id}`,
    group: "Settings",
    label: project.name,
    keys: null,
    meta: "Settings",
    lead: { kind: "none" } as const,
    run: go(`project:${project.id}`),
  }));
  return [...own, ...projects];
}

/** What the palette shows for `typed`, each group narrowed and in its order. */
export function catalogue(app: App, typed: string, found: readonly Ticket[]): PaletteItem[] {
  const here = actionsHere(app);
  const text = (item: PaletteItem) =>
    item.lead.kind === "ticket" ? `${item.lead.key} ${item.label}` : item.label;
  return [
    here,
    anywhere(app, here),
    sessions(app),
    tickets(app, found),
    settings(app, typed),
  ].flatMap((group) => narrowed(group, typed, text));
}
