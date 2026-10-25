// The pane head's action for what stands beside the session (design
// system: PaneHeader): the shell, an icon pressed while one of the
// workspace's shells is placed beside it, with its key in its tooltip; it
// places one, or gives the one there the keys. Full screen is in the pane's
// more menu, on its key.

import { useApp, useDaemon } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { useKeyLabel } from "../keys/useKeymap";
import { IconButton } from "../ui/controls";
import { openShell, running } from "./beside";
import { useLayout } from "./useLayout";

export function ShellButton({ session }: { session: SessionId }) {
  const app = useApp();
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const placed = useLayout().layout.placed();
  // Pressed while one of the workspace's shells is placed beside it.
  const open = useDaemon((state) =>
    placed.some((id) => {
      const shell = state.sessions[id];
      return shell?.workspace === workspace && shell.harness === "shell" && running(shell);
    }),
  );
  const key = useKeyLabel("shell");
  return (
    <IconButton
      icon="terminal"
      label="Shell"
      title={open ? `The shell here (${key})` : `Shell in this workspace (${key})`}
      pressed={open}
      onClick={() => {
        if (workspace) void openShell(app, workspace, session);
      }}
    />
  );
}
