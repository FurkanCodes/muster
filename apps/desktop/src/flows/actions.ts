// What the flows' keys do on what is selected, as the TUI's do
// (crates/muster-tui/src/app/events.rs, rename.rs, tidy.rs, stacks.rs): a
// dialog where the person must answer, the command at once where nothing
// is lost (removing an ended session, restarting one), and where neither
// can be, the TUI's words saying why, in the status bar.

import type { App } from "../app/context";
import type { Project } from "../ipc/generated/Project";
import type { Session } from "../ipc/generated/Session";
import type { Workspace } from "../ipc/generated/Workspace";
import { projectsInOrder } from "../state/selectors";
import type { ScreenId } from "../state/ui";
import { whenKnown } from "../state/whenKnown";
import { labelOf } from "./keymap";

/** The screens whose lines are sessions, worktrees and projects. */
export const SESSION_LISTS: readonly ScreenId[] = ["all", "needs-you"];

/** A session's terminal at the size the pane starts it at; it settles to the pane's own on show. */
export const START_SIZE = { cols: 100, rows: 30 };

/** What is selected on the screen shown: a session, a worktree or a project, and what holds it. */
export interface Here {
  session: Session | null;
  workspace: Workspace | null;
  project: Project | null;
  /** A project's line itself, not one of its worktrees or sessions. */
  onProject: boolean;
}

export function here(app: App): Here {
  const ui = app.ui.getState();
  const state = app.daemon.store.getState();
  const none: Here = { session: null, workspace: null, project: null, onProject: false };
  if (!SESSION_LISTS.includes(ui.screen)) return none;
  const id = ui.selected[ui.screen];
  if (!id) return none;
  const session = state.sessions[id] ?? null;
  const workspace = state.workspaces[session?.workspace ?? id] ?? null;
  const project = state.projects[workspace?.project ?? id] ?? null;
  return { session, workspace, project, onProject: !session && !workspace && project !== null };
}

function complain(app: App, text: string): void {
  app.actions.tell(text, true);
}

function key(app: App, action: Parameters<typeof labelOf>[1]): string {
  return labelOf(app.daemon.store.getState().preferences, action);
}

/** Whether a session still runs: it has not ended. */
export function runs(session: Session): boolean {
  return session.ended_at === null;
}

/** A plain folder's workspace: no git, so no branch. */
export function plain(workspace: Workspace): boolean {
  return workspace.kind.kind !== "container" && workspace.kind.branch === "";
}

function checkoutOf(app: App, project: string): Workspace | null {
  return (
    Object.values(app.daemon.store.getState().workspaces).find(
      (workspace) => workspace.project === project && workspace.kind.kind === "checkout",
    ) ?? null
  );
}

/**
 * `a`: the Mac's own folder panel, the project added as it is chosen;
 * where the app has none (its tests' build, the mock), the folder browser.
 */
export function addProject(app: App): void {
  const browse = () => app.actions.openFlow({ kind: "add-project" });
  app.bridge.pickFolder().then((picked) => {
    if (picked.kind === "unavailable") return browse();
    if (picked.kind === "chosen")
      addProjectAt(app, picked.path).catch((error: unknown) => complain(app, said(error)));
  }, browse);
}

/** `g`: a project cloned from GitHub, into a folder the person chooses. */
export function cloneProject(app: App): void {
  app.actions.openFlow({ kind: "clone-project" });
}

/**
 * `n` (and ⌘N): the selected project's page with its box ready to start
 * work, in the session's or worktree's place where one is selected, else in
 * a new worktree; the first project's where none is selected (Tickets,
 * Settings). Work starts in one place only: the project's page.
 */
export function newSession(app: App): void {
  const state = app.daemon.store.getState();
  const { workspace, project } = here(app);
  const into = project ?? projectsInOrder(state)[0];
  if (!into) {
    complain(app, `Add a project first: press ${key(app, "add_project")}.`);
    return;
  }
  // A run's container is the run's own: work starts elsewhere.
  const place = workspace && workspace.kind.kind !== "container" ? workspace.id : null;
  app.actions.compose(into.id, place);
}

/** `w`: a new worktree for the selected project. */
export function newWorktree(app: App): void {
  const { project } = here(app);
  if (!project) {
    complain(app, `Add a project first: press ${key(app, "add_project")}.`);
    return;
  }
  const checkout = checkoutOf(app, project.id);
  if (checkout && plain(checkout)) {
    complain(
      app,
      `${project.name} is a plain folder, not a git repository, so it has no worktrees: press ${key(app, "new_session")} for a session in it.`,
    );
    return;
  }
  app.actions.openFlow({ kind: "new-worktree", project: project.id });
}

/**
 * `R`, where no list answers it: the lists rename a session in place
 * (src/session/useSessionActions.tsx), so here only nothing is selected.
 */
export function rename(app: App): void {
  if (!here(app).session) complain(app, "Select a session to rename it.");
}

/**
 * `x` on a worktree or a project, after asking; a session's is the lists'
 * own (src/session/useSessionActions.tsx).
 */
export function close(app: App): void {
  const { session, workspace, project, onProject } = here(app);
  if (onProject && project) {
    app.actions.openFlow({ kind: "remove-project", project: project.id });
    return;
  }
  if (session) return;
  if (!workspace) {
    complain(app, "Select a worktree or an ended session to remove.");
    return;
  }
  if (workspace.kind.kind === "checkout") {
    complain(
      app,
      "That is the project's own checkout, which Muster never removes; select the project to stop tracking it.",
    );
    return;
  }
  if (workspace.kind.kind === "container") {
    complain(
      app,
      "That is a run's container, which goes when the run ends; cancel the run to end it.",
    );
    return;
  }
  app.actions.openFlow({ kind: "remove-worktree", workspace: workspace.id });
}

/**
 * The selected session's worktree removed, after asking: from the palette,
 * as `x` on a session closes the session itself.
 */
export function removeWorktree(app: App): void {
  const { workspace } = here(app);
  if (workspace?.kind.kind === "worktree")
    app.actions.openFlow({ kind: "remove-worktree", workspace: workspace.id });
}

/** `s`: stops the selected session, after asking. */
export function stop(app: App): void {
  const { session } = here(app);
  if (!session) {
    complain(app, "Select a session to stop.");
    return;
  }
  if (!runs(session)) {
    complain(app, "That session has already ended.");
    return;
  }
  app.actions.openFlow({ kind: "stop", session: session.id });
}

/** `r`: starts an ended session again, and shows it. */
export function restart(app: App): void {
  const { session } = here(app);
  if (!session) {
    complain(app, "Select a session to restart.");
    return;
  }
  if (runs(session)) {
    complain(app, `${session.title} is still running.`);
    return;
  }
  app.bridge.command({ RestartSession: { session: session.id, size: START_SIZE } }).then(
    () => app.actions.showTerminal(session.id),
    (error: unknown) => complain(app, said(error)),
  );
}

/** The worktree selected for work by hand, or why there is none. */
export function handsOn(app: App, what: string): Workspace | null {
  const { workspace, onProject } = here(app);
  if (!workspace || onProject) {
    complain(app, `Select a session in a worktree to ${what}.`);
    return null;
  }
  if (workspace.kind.kind === "container") {
    complain(app, "A run does that itself; hand it back to have it done.");
    return null;
  }
  return workspace;
}

/** `t`: which agent tidies the selected worktree's commits. */
export function tidy(app: App): void {
  const workspace = handsOn(app, "tidy its commits");
  if (!workspace) return;
  if (plain(workspace)) {
    complain(app, "It is not a git repository, so it has no commits to tidy.");
    return;
  }
  app.actions.openFlow({ kind: "tidy", workspace: workspace.id });
}

/** `u`: moves the selected worktree's branch onto its base, after asking. */
export function moveBranch(app: App): void {
  const workspace = handsOn(app, "move its branch onto its base");
  if (!workspace) return;
  if (plain(workspace)) {
    complain(app, "It is not a git repository, so it has no branch to move.");
    return;
  }
  app.actions.openFlow({ kind: "move", workspace: workspace.id });
}

/** Adds the project at `path`, says so, and selects it. */
export async function addProjectAt(app: App, path: string): Promise<void> {
  const reply = await app.bridge.command({ AddProject: { root: path, name: null } });
  if (typeof reply !== "object" || !("ProjectAdded" in reply))
    throw new Error("The daemon answered something else; try again.");
  const id = reply.ProjectAdded.id;
  app.actions.show("all");
  whenKnown(app, "projects", id, () => {
    app.actions.select("all", id);
    app.actions.tell(`Added ${app.daemon.store.getState().projects[id]?.name ?? path}.`);
  });
}

/** What a refused command says. */
export function said(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
