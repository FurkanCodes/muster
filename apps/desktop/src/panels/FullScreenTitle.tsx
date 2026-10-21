// The title bar while a session fills the window (the FullScreen board):
// its agent's logo, its name, its branch and project; and the way back.

import { useActions, useDaemon } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { branchOf, projectColour } from "../state/selectors";
import { IconButton } from "../ui/controls";
import { AgentLogo } from "../ui/status";
import { writtenKeys } from "./usePanelKeys";

export function FullScreenTitle({ session }: { session: SessionId }) {
  const title = useDaemon((state) => state.sessions[session]?.title ?? "");
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[state.sessions[session]?.workspace ?? ""];
    return workspace ? branchOf(workspace) : "";
  });
  const projectId = useDaemon(
    (state) => state.workspaces[state.sessions[session]?.workspace ?? ""]?.project ?? "",
  );
  const project = useDaemon((state) => state.projects[projectId]?.name ?? "");
  const colour = useDaemon((state) => projectColour(state, projectId));
  return (
    <>
      <AgentLogo harness={harness} size="sm" />
      <span className="mu-cut panels-fullscreen-title" title={title}>
        {title}
      </span>
      <span className="mu-branch mu-quiet">⎇ {branch}</span>
      <span className="mu-where">
        in <span className={`mu-p${colour}`}>{project}</span>
      </span>
      <span className="mu-spacer" data-tauri-drag-region />
    </>
  );
}

export function LeaveFullScreen() {
  const actions = useActions();
  const key = useDaemon((state) => writtenKeys(state.preferences?.keys, "full_screen", ["f"])[0]);
  return (
    <IconButton
      icon="collapse"
      label="Leave full screen"
      title={`Leave full screen (${key})`}
      onClick={() => actions.fillWindow(null)}
    />
  );
}
