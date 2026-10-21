// The pane head's action for what stands beside the session (design
// system: PaneHeader): the shell, an icon pressed while the workspace's
// shell is beside it, with its key in its tooltip. Full screen is in the
// pane's more menu, on its key.

import { useApp, useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { useKeyLabel } from "../keys/useKeymap";
import { IconButton } from "../ui/controls";
import { shellBeside, toggleShell } from "./beside";

export function ShellButton({ session }: { session: SessionId }) {
  const app = useApp();
  const kept = useUi((state) => state.shell);
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const open = useDaemon((state) => {
    const shell = shellBeside(state, kept, session);
    return Boolean(shell && state.sessions[shell]?.workspace === workspace);
  });
  const key = useKeyLabel("shell");
  return (
    <IconButton
      icon="terminal"
      label="Shell"
      title={open ? `Put the shell away (${key})` : `Shell in this workspace (${key})`}
      pressed={open}
      onClick={() => {
        if (workspace) void toggleShell(app, workspace, session);
      }}
    />
  );
}
