// The pane head's actions for what stands beside the session (design
// system: PaneHeader): *Shell*, pressed while the workspace's shell is
// beside it, and full screen, each with its key in its tooltip.

import { useApp, useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { useKeyLabel } from "../keys/useKeymap";
import { Button, IconButton } from "../ui/controls";
import { shellBeside, toggleShell } from "./beside";
import { toggleFullScreen } from "./usePanelKeys";

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
    <Button
      title={open ? `Put the shell away (${key})` : `Shell in this workspace (${key})`}
      aria-pressed={open}
      onClick={() => {
        if (workspace) void toggleShell(app, workspace, session);
      }}
    >
      <span className="mu-mono" aria-hidden="true">
        ❯
      </span>
      Shell
    </Button>
  );
}

export function FullScreenButton({ session }: { session: SessionId }) {
  const app = useApp();
  const key = useKeyLabel("full_screen");
  return (
    <IconButton
      icon="expand"
      label="Full screen"
      title={`Full screen (${key})`}
      onClick={() => toggleFullScreen(app, session)}
    />
  );
}
