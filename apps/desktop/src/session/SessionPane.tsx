// The session beside the list (design system: PaneHeader, NeedsYouBanner,
// Terminal): one row at the window's top with its name, where it runs and
// what its work comes to, and as icons its shell, its changes and the rest
// of what can be done with it; over the terminal what it waits for while it
// asks; then its live terminal, with the shell and plugins' panels beside
// it (src/panels), or the workspace's changes in its place.

import { lazy, Suspense } from "react";
import { useActions, useApp, useDaemon, useUi } from "../app/context";
import { KeysHeld } from "../app/keys";
import { Page, PageActions, PageHeader, PageMeta, PageTitle } from "../app/Page";
import type { SessionId } from "../ipc/generated/SessionId";
import { KEY_SCREENS, useActionKeys, useKeyLabel, useKeymap } from "../keys/useKeymap";
import { ShellButton } from "../panels/PaneActions";
import { Panels } from "../panels/Panels";
import { toggleFullScreen } from "../panels/usePanelKeys";
import { branchOf, projectColour } from "../state/selectors";
import { IconButton } from "../ui/controls";
import { ActionMenu, keysOf, type MenuItem } from "../ui/Menu";
import { AgentLogo, Branch, DiffStat, PullRequest } from "../ui/status";
import { browsePullRequest } from "./byHand";
import { NeedsYouBanner } from "./NeedsYouBanner";
import "./session.css";

const ChangesView = lazy(() => import("./changes/ChangesView"));

/**
 * "⎇ feature/x in muster +120 −30 overlap #88": where it runs, and what its
 * work there comes to, each a way on: the branch to its project's page, what
 * the work comes to and where it meets other work to its changes, the pull
 * request to the browser.
 */
function Where({ session, onChanges }: { session: SessionId; onChanges: () => void }) {
  const app = useApp();
  const actions = useActions();
  const changesKey = useKeyLabel("show_changes");
  const browserKey = useKeyLabel("pull_request_in_browser");
  const projectId = useDaemon(
    (state) => state.workspaces[state.sessions[session]?.workspace ?? ""]?.project ?? null,
  );
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
    <PageMeta>
      <button
        type="button"
        className="app-head-link"
        title={project ? `${project}'s page` : undefined}
        disabled={!projectId}
        onClick={() => {
          if (!projectId) return;
          actions.select("all", projectId);
          actions.show("all");
        }}
      >
        <Branch branch={branch} project={project} colour={colour} />
      </button>
      {added || removed ? (
        <button
          type="button"
          className="app-head-link"
          title={`See what changed (${changesKey})`}
          onClick={onChanges}
        >
          <DiffStat added={added} removed={removed} />
        </button>
      ) : null}
      {clash ? (
        <button
          type="button"
          className="app-head-link"
          title={`See where it meets other work (${changesKey})`}
          onClick={onChanges}
        >
          <span className={`mu-clash-${clash}`}>{clash}</span>
        </button>
      ) : null}
      {pr ? (
        <button
          type="button"
          className="app-head-link"
          title={`#${pr.number} in the browser (${browserKey})`}
          onClick={() => browsePullRequest(app)}
        >
          <PullRequest pr={pr} />
        </button>
      ) : null}
    </PageMeta>
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
  // Asked for by a right-click on this session's line in the list.
  const menuAt = useUi((state) => (state.menuAt?.session === session ? state.menuAt : null));
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
    <Page label={title} className="session-pane">
      <PageHeader>
        <AgentLogo harness={harness} />
        <PageTitle title={title} className={hasKeys ? "app-pane-title--keys" : undefined}>
          {title}
        </PageTitle>
        <Where session={session} onChanges={changes} />
        <PageActions>
          <ShellButton session={session} />
          <IconButton
            icon="changes"
            label="Changes"
            title={`Changes (${changesKey})`}
            pressed={mode === "changes"}
            onClick={changes}
          />
          <ActionMenu
            label={title}
            items={items}
            trigger={<IconButton icon="more" label="More" />}
          />
        </PageActions>
        {menuAt ? (
          <ActionMenu
            label={title}
            items={items}
            at={menuAt}
            onClose={() => actions.openMenuAt(null)}
          />
        ) : null}
      </PageHeader>
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
    </Page>
  );
}
