// The session beside the list (design system: PaneHeader, NeedsYouBanner,
// Terminal): one row at the window's top with its name, where it runs and
// what its work comes to, and as icons its shell, its changes and the rest
// of what can be done with it; over the terminal what it waits for while it
// asks; then its live terminal, with the shell and plugins' panels beside
// it (src/panels), or the workspace's changes in its place.

import { lazy, Suspense, useState } from "react";
import { useActions, useApp, useDaemon, useUi } from "../app/context";
import { KeysHeld } from "../app/keys";
import { SidebarToggle } from "../app/Sidebar";
import type { SessionId } from "../ipc/generated/SessionId";
import { KEY_SCREENS, useActionKeys, useKeyLabel, useKeymap } from "../keys/useKeymap";
import { ShellButton } from "../panels/PaneActions";
import { Panels } from "../panels/Panels";
import { toggleFullScreen } from "../panels/usePanelKeys";
import { branchOf, projectColour } from "../state/selectors";
import { IconButton } from "../ui/controls";
import { AgentLogo, Branch, DiffStat, PullRequest } from "../ui/status";
import { NeedsYouBanner } from "./NeedsYouBanner";
import { keysOf, type MenuItem, SessionMenu } from "./SessionMenu";
import "./session.css";

const ChangesView = lazy(() => import("./changes/ChangesView"));

/** The window's own buttons sit over the pane's head while the sidebar is away: room for them, and the way back. */
export function SidebarAway() {
  const away = useUi((state) => state.sidebarFolded || state.fullScreen !== null);
  return away ? (
    <span className="app-head-away">
      <SidebarToggle />
    </span>
  ) : null;
}

/** "⎇ feature/x in muster +120 −30 overlap #88": where it runs, and what its work there comes to. */
function Where({ session }: { session: SessionId }) {
  const workspaceId = useDaemon((state) => state.sessions[session]?.workspace ?? "");
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[workspaceId];
    return workspace ? branchOf(workspace) : "";
  });
  const project = useDaemon((state) => {
    const projectId = state.workspaces[workspaceId]?.project;
    return projectId ? (state.projects[projectId]?.name ?? "") : "";
  });
  const colour = useDaemon((state) => {
    const projectId = state.workspaces[workspaceId]?.project;
    return projectId ? projectColour(state, projectId) : 1;
  });
  const added = useDaemon((state) => state.workspaces[workspaceId]?.changes?.added ?? 0);
  const removed = useDaemon((state) => state.workspaces[workspaceId]?.changes?.removed ?? 0);
  const clash = useDaemon((state) => {
    const clashes = state.workspaces[workspaceId]?.clashes ?? [];
    if (clashes.length === 0) return null;
    return clashes.some((one) => one.conflicts > 0) ? "conflict" : "overlap";
  });
  const pr = useDaemon((state) => state.workspaces[workspaceId]?.pull_request ?? null);
  return (
    <span className="mu-pane-meta app-head-where">
      <Branch branch={branch} project={project} colour={colour} />
      {added || removed ? <DiffStat added={added} removed={removed} /> : null}
      {clash ? <span className={`mu-clash-${clash}`}>{clash}</span> : null}
      {pr ? <PullRequest pr={pr} /> : null}
    </span>
  );
}

export function SessionPane({
  session,
  menu = [],
}: {
  session: SessionId;
  /** What the list's keys do to the session, offered again in the more menu after full screen. */
  menu?: readonly MenuItem[];
}) {
  const app = useApp();
  const actions = useActions();
  const map = useKeymap();
  const [menuOpen, setMenuOpen] = useState(false);
  const fullScreen = useUi((state) => state.fullScreen === session);
  const title = useDaemon((state) => state.sessions[session]?.title ?? "");
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const mode = useUi((state) => state.pane);
  const screen = useUi((state) => KEY_SCREENS[state.screen]);
  const changesKey = useKeyLabel("show_changes");
  // The list has the keys in the changes, and the terminal its own again after.
  const changes = () => {
    actions.keysTo("list");
    actions.showPane(mode === "changes" ? "terminal" : "changes");
  };
  useActionKeys(screen, { show_changes: changes });
  const items: MenuItem[] = [
    {
      ...keysOf(map, "full_screen"),
      label: fullScreen ? "Leave full screen" : "Full screen",
      does: () => toggleFullScreen(app, session),
    },
    ...menu,
  ];
  return (
    <section className="mu-pane app-pane session-pane" aria-label={title}>
      <header className="app-head" data-tauri-drag-region>
        <SidebarAway />
        <AgentLogo harness={harness} />
        <h2 className={`app-head-title ${hasKeys ? "app-pane-title--keys" : ""}`} title={title}>
          {title}
        </h2>
        <Where session={session} />
        <span className="mu-spacer" data-tauri-drag-region />
        <div className="mu-pane-actions">
          <ShellButton session={session} />
          <IconButton
            icon="changes"
            label="Changes"
            title={`Changes (${changesKey})`}
            pressed={mode === "changes"}
            onClick={changes}
          />
          <IconButton
            icon="more"
            label="More"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen(!menuOpen)}
          />
        </div>
        {menuOpen ? (
          <SessionMenu label={title} items={items} onClose={() => setMenuOpen(false)} />
        ) : null}
      </header>
      {mode === "changes" && workspace ? (
        <Suspense fallback={null}>
          {/* Once shown, the changes hold the keys: one they do not answer never reaches the list. */}
          <KeysHeld>
            <ChangesView key={workspace} workspace={workspace} />
          </KeysHeld>
        </Suspense>
      ) : (
        <>
          <NeedsYouBanner session={session} />
          <div className="app-pane-terminal session-terminal">
            <Panels session={session} />
          </div>
        </>
      )}
    </section>
  );
}
