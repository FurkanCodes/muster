// What fills the window beside the sidebar's list: the line it has selected,
// whichever list shows. A session's pane, a project's page, a worktree no
// session is in, or the place kept for one. Drawn once by the window, not
// by each list, so switching between Needs you and All keeps the pane, and
// its terminal, where both have the same session selected.

import { useKeyLabel } from "../keys/useKeymap";
import { ProjectPage } from "../screens/sessions/ProjectPage";
import { RowDetail } from "../screens/sessions/RowDetail";
import { SessionPane } from "../session/SessionPane";
import { useSessionMenu } from "../session/useSessionMenu";
import { useDaemon, useUi } from "./context";
import { NothingSelected } from "./Page";

type Shown = "session" | "project" | "workspace" | null;

export function Detail() {
  const id = useUi((state) => state.selected[state.list]);
  const shown = useDaemon((state): Shown => {
    if (!id) return null;
    if (state.sessions[id]) return "session";
    if (state.projects[id]) return "project";
    if (state.workspaces[id]) return "workspace";
    return null;
  });
  const menu = useSessionMenu(shown === "session" ? id : undefined);
  const shellKey = useKeyLabel("shell");
  if (!id || shown === null) return <NothingSelected className="app-detail-none" />;
  if (shown === "session") return <SessionPane session={id} menu={menu} />;
  if (shown === "project") return <ProjectPage key={id} project={id} />;
  return <RowDetail workspace={id} shellKey={shellKey} />;
}
