// Starting work, as the new session dialog (NewSession.tsx) and a
// project's page (screens/sessions/ProjectPage.tsx) both do: the daemon's
// own commands, CreateWorktree then StartSession, and then the session
// opened, its terminal with the keys.

import type { App } from "../app/context";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { whenKnown } from "../state/whenKnown";
import { START_SIZE } from "./actions";

/** A worktree of `project` on `branch`, or on a new one of Muster's naming. */
export async function makeWorktree(
  app: App,
  project: ProjectId,
  branch: string | null,
): Promise<WorkspaceId> {
  const reply = await app.bridge.command({
    CreateWorktree: { project, branch: branch?.trim() || null, on: null, named_for: null },
  });
  if (typeof reply !== "object" || !("WorktreeCreated" in reply))
    throw new Error("The daemon answered something else; try again.");
  return reply.WorktreeCreated.id;
}

/** `harness` started in `workspace`, with `prompt` first where it takes one. */
export async function startSession(
  app: App,
  workspace: WorkspaceId,
  harness: string,
  prompt: string | null,
): Promise<SessionId> {
  const reply = await app.bridge.command({
    StartSession: {
      workspace,
      harness,
      title: null,
      size: START_SIZE,
      prompt,
      started_by: null,
    },
  });
  if (typeof reply !== "object" || !("SessionStarted" in reply))
    throw new Error("The daemon answered something else; try again.");
  return reply.SessionStarted.id;
}

/** The session just started, selected in the list with its terminal and the keys. */
export function openStarted(app: App, session: SessionId, harness: string): void {
  app.actions.choseAgent(harness);
  app.actions.show("all");
  whenKnown(app, "sessions", session, () => {
    app.actions.select("all", session);
    app.actions.showTerminal(session);
    app.actions.keysTo("terminal");
  });
}
