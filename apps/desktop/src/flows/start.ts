// Starting work, as the new session dialog (NewSession.tsx) and a
// project's page (screens/sessions/ProjectPage.tsx) both do: the daemon's
// own commands, CreateWorktree then StartSession, and then the session
// opened, its terminal with the keys.

import type { App } from "../app/context";
import { ask } from "../ipc/ask";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { errorText } from "../lib/text";
import { whenKnown } from "../state/whenKnown";
import { START_SIZE } from "./actions";

/** A worktree of `project` on `branch`, or on a new one of Muster's naming. */
export async function makeWorktree(
  app: App,
  project: ProjectId,
  branch: string | null,
): Promise<WorkspaceId> {
  const reply = await ask(
    app.bridge,
    {
      CreateWorktree: { project, branch: branch?.trim() || null, on: null, named_for: null },
    },
    "WorktreeCreated",
  );
  return reply.id;
}

/** `harness` started in `workspace`, with `prompt` first where it takes one. */
export async function startSession(
  app: App,
  workspace: WorkspaceId,
  harness: string,
  prompt: string | null,
): Promise<SessionId> {
  const reply = await ask(
    app.bridge,
    {
      StartSession: {
        workspace,
        harness,
        title: null,
        size: START_SIZE,
        space: null,
        prompt,
        started_by: null,
      },
    },
    "SessionStarted",
  );
  return reply.id;
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

/**
 * `harness` started in `project` at once, from its line's +: an agent in a
 * new worktree, as the project's page starts one, or in the folder itself
 * for a plain folder; a shell in its checkout. Then opened.
 */
export async function startIn(app: App, project: ProjectId, harness: string): Promise<void> {
  const state = app.daemon.store.getState();
  const checkout = Object.values(state.workspaces).find(
    (workspace) => workspace.project === project && workspace.kind.kind === "checkout",
  );
  if (!checkout) return app.actions.tell("The project has no checkout to start in.", true);
  const plain = checkout.kind.branch === "";
  try {
    const workspace =
      harness === "shell" || plain ? checkout.id : await makeWorktree(app, project, null);
    const session = await startSession(app, workspace, harness, null);
    openStarted(app, session, harness);
  } catch (error) {
    app.actions.tell(errorText(error), true);
  }
}
