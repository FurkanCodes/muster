// The dialogs the flows open over the window: the window's own state, never
// the daemon's. Any screen opens one with `useActions().openFlow(...)`.

import type { Member } from "../ipc/generated/Member";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { SpaceId } from "../ipc/generated/SpaceId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";

export type Flow =
  | { kind: "add-project" }
  /** A project cloned from GitHub, into a folder the person chooses. */
  | { kind: "clone-project" }
  | { kind: "new-worktree"; project: ProjectId }
  | { kind: "stop"; session: SessionId }
  | { kind: "tidy"; workspace: WorkspaceId }
  | { kind: "move"; workspace: WorkspaceId }
  | { kind: "remove-worktree"; workspace: WorkspaceId }
  | { kind: "remove-project"; project: ProjectId }
  /** A new space's name (`space` null), `join` put in it once made; or a space's new name. */
  | { kind: "space-name"; space: SpaceId | null; join?: Member }
  | { kind: "remove-space"; space: SpaceId }
  /** Where a terminal in no project goes under a project (`ARC-33`). */
  | { kind: "file-terminal"; session: SessionId };
