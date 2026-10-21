// What the mock daemon derives from its state: the same rules the Rust side
// takes from the shared model (`muster-model`), kept to what tests need.
// The real app never runs this: its derivations come from Rust.

import type { Derived } from "../generated/desktop/Derived";
import type { Row } from "../generated/desktop/Row";
import type { Project } from "../generated/Project";
import type { Session } from "../generated/Session";
import type { SessionId } from "../generated/SessionId";
import type { Workspace } from "../generated/Workspace";

/** A shell, or a plugin's panel: the person's own or a plugin's, never an agent's chat. */
export function standsBeside(harness: string): boolean {
  return harness === "shell" || harness.startsWith("plugin:");
}

/** Whether a session waits on the person (`muster_core::news::needs_attention`). */
export function needsAttention(session: Session): boolean {
  return !standsBeside(session.harness) && (session.status.state === "needs_you" || session.unseen);
}

export interface World {
  projects: Project[];
  workspaces: Workspace[];
  sessions: Session[];
}

/** A project's workspaces: its checkout first, then worktrees oldest first, then containers. */
export function workspacesOf(world: World, project: string): Workspace[] {
  const rank = (workspace: Workspace) =>
    workspace.kind.kind === "checkout" ? 0 : workspace.kind.kind === "worktree" ? 1 : 2;
  return world.workspaces
    .filter((workspace) => workspace.project === project)
    .sort((a, b) => rank(a) - rank(b) || a.created_at - b.created_at);
}

/** A workspace's sessions as listed: its chats, then its shells and panels. */
export function listedOf(world: World, workspace: string): Session[] {
  const mine = world.sessions.filter((session) => session.workspace === workspace);
  return [
    ...mine.filter((session) => !standsBeside(session.harness)),
    ...mine.filter((session) => standsBeside(session.harness)),
  ];
}

export function derive(world: World, changed: ReadonlyMap<SessionId, number>): Derived {
  const asking = (session: Session) => session.status.state === "needs_you";
  const needsYou = world.sessions
    .filter(needsAttention)
    .sort(
      (a, b) =>
        Number(!asking(a)) - Number(!asking(b)) ||
        (changed.get(a.id) ?? 0) - (changed.get(b.id) ?? 0),
    )
    .map((session) => session.id);

  const rows: Row[] = [];
  for (const project of world.projects) {
    rows.push({ kind: "project", id: project.id });
    for (const workspace of workspacesOf(world, project.id)) {
      rows.push({ kind: "workspace", id: workspace.id });
      for (const session of listedOf(world, workspace.id)) {
        rows.push({ kind: "session", id: session.id });
      }
    }
  }

  const chats = world.sessions.filter((session) => !standsBeside(session.harness));
  return {
    needs_you: needsYou,
    rows,
    counts: {
      working: chats.filter((session) => session.status.state === "running").length,
      waiting: chats.filter(asking).length,
      done: chats.filter((session) => session.status.state === "finished").length,
    },
  };
}
