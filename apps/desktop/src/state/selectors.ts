// Questions the screens ask of the daemon's state. Orderings that need the
// model's memory come from `derived`; these only look things up.

import type { Project } from "../ipc/generated/Project";
import type { Session } from "../ipc/generated/Session";
import type { Workspace } from "../ipc/generated/Workspace";
import type { DaemonState } from "./daemon";

export function standsBeside(session: Pick<Session, "harness">): boolean {
  return session.harness === "shell" || session.harness.startsWith("plugin:");
}

/** Projects in the order the daemon lists them. */
export function projectsInOrder(state: DaemonState): Project[] {
  const out: Project[] = [];
  for (const row of state.derived.rows) {
    if (row.kind === "project") {
      const project = state.projects[row.id];
      if (project) out.push(project);
    }
  }
  return out;
}

/** A project's colour, 1 to 6, by its place in the list (README: Colour). */
export function projectColour(state: DaemonState, projectId: string): number {
  const at = projectsInOrder(state).findIndex((project) => project.id === projectId);
  return at < 0 ? 1 : (at % 6) + 1;
}

export function workspaceOf(state: DaemonState, session: Session): Workspace | undefined {
  return state.workspaces[session.workspace];
}

export function projectOf(state: DaemonState, session: Session): Project | undefined {
  const workspace = workspaceOf(state, session);
  return workspace ? state.projects[workspace.project] : undefined;
}

/** A workspace's branch, or the folder's name for a plain folder. */
export function branchOf(workspace: Workspace): string {
  return workspace.kind.branch || "no branch";
}

/** Chats, as the counts and queues count them: not shells, not plugins' panels. */
export function chats(state: DaemonState): Session[] {
  return Object.values(state.sessions).filter((session) => !standsBeside(session));
}
