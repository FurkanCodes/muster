// The dialogs the flows open over the window: the window's own state, never
// the daemon's. Any screen opens one with `useActions().openFlow(...)`.

import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";

export type Flow =
  | { kind: "add-project" }
  /** Where it was opened: a workspace to start in, or a project to start for. */
  | { kind: "new-session"; project: ProjectId | null; workspace: WorkspaceId | null }
  | { kind: "new-worktree"; project: ProjectId }
  | { kind: "rename"; session: SessionId }
  | { kind: "close"; session: SessionId }
  | { kind: "stop"; session: SessionId }
  | { kind: "tidy"; workspace: WorkspaceId }
  | { kind: "move"; workspace: WorkspaceId }
  | { kind: "remove-worktree"; workspace: WorkspaceId }
  | { kind: "remove-project"; project: ProjectId };
