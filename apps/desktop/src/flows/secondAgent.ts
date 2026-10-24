// Before another agent starts where one already works: both would change
// the same files. The same rule and words as the terminal UI's
// (`muster_model::Model::second_agent_warning`).

import type { Session } from "../ipc/generated/Session";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { oneLine } from "../session/changes/labels";
import { shownHarness } from "../state/selectors";

/** `SessionKind::is_agent`: a coding agent's chat, not a shell, a plugin's panel or the checks. */
export function isAgent(harness: string): boolean {
  return harness !== "shell" && harness !== "checks" && !/^plugin:[^/]+\/[^/]+$/.test(harness);
}

/** The agents whose program still runs in `workspace`, oldest first. */
export function agentsAliveIn(
  sessions: Readonly<Record<string, Session>>,
  workspace: WorkspaceId,
): Session[] {
  return Object.values(sessions)
    .filter(
      (session) =>
        session.workspace === workspace &&
        isAgent(shownHarness(session)) &&
        session.status.state !== "exited",
    )
    .sort((a, b) => a.created_at - b.created_at || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** What the person is told before starting another agent in `workspace`, if one works there. */
export function secondAgentWarning(
  sessions: Readonly<Record<string, Session>>,
  workspace: WorkspaceId,
): string | null {
  const agents = agentsAliveIn(sessions, workspace);
  const [first] = agents;
  if (!first) return null;
  const who =
    agents.length === 1
      ? `${oneLine(first.title)} already works here`
      : `${agents.length} agents already work here`;
  return `${who}: two agents in one place change the same files. A new worktree keeps their work apart.`;
}
