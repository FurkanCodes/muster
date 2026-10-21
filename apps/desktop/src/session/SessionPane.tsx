// The detail pane beside a list (design system: PaneHeader, NeedsYouBanner,
// Terminal, ThisTurn): which session, where it runs and what it uses, what
// it waits for while it asks, its live terminal, and what its newest turn
// changed, the shell and plugins' panels beside the terminal (src/panels).
// The changes branch fills ChangesView.

import { lazy, Suspense, useState } from "react";
import { useActions, useDaemon, useUi } from "../app/context";
import { useKeys } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import { agentOf } from "../lib/agents";
import { spend } from "../lib/format";
import { FullScreenButton, ShellButton } from "../panels/PaneActions";
import { Panels } from "../panels/Panels";
import { branchOf, projectColour } from "../state/selectors";
import { Button, IconButton } from "../ui/controls";
import { AgentLogo, Branch, PullRequest } from "../ui/status";
import { NeedsYouBanner } from "./NeedsYouBanner";
import { type MenuItem, SessionMenu } from "./SessionMenu";
import { ThisTurn } from "./ThisTurn";
import "./session.css";

const ChangesView = lazy(() => import("./changes/ChangesView"));

/** "212 MB, 3% CPU": what the session's processes use, as last measured. */
function Usage({ session }: { session: SessionId }) {
  const usage = useDaemon((state) => {
    const found = state.resources?.sessions.find((measured) => measured.session === session);
    return found
      ? `${Math.round(found.memory_bytes / 1e6)} MB, ${Math.round(found.cpu_permille / 10)}% CPU`
      : null;
  });
  return usage ? <span>{usage}</span> : null;
}

function PaneMeta({ session }: { session: SessionId }) {
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const tokens = useDaemon((state) => state.sessions[session]?.spend?.tokens ?? null);
  const cost = useDaemon((state) => state.sessions[session]?.spend?.cost_micros ?? null);
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
  const pr = useDaemon((state) => state.workspaces[workspaceId]?.pull_request ?? null);
  return (
    <div className="mu-pane-meta">
      <Branch branch={branch} project={project} colour={colour} />
      <span>{agentOf(harness).name}</span>
      {tokens === null ? null : <span>{spend(tokens, cost)}</span>}
      <Usage session={session} />
      {pr ? <PullRequest pr={pr} /> : null}
    </div>
  );
}

export function SessionPane({
  session,
  menu = [],
}: {
  session: SessionId;
  /** What the list's keys do to the session, offered again in the more menu. */
  menu?: readonly MenuItem[];
}) {
  const actions = useActions();
  const [menuOpen, setMenuOpen] = useState(false);
  const title = useDaemon((state) => state.sessions[session]?.title ?? "");
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const mode = useUi((state) => state.pane);
  // The list has the keys in the changes, and the terminal its own again after.
  const changes = () => {
    actions.keysTo("list");
    actions.showPane(mode === "changes" ? "terminal" : "changes");
  };
  useKeys({ d: changes });
  return (
    <section className="mu-pane app-pane session-pane" aria-label={title}>
      <div className="mu-pane-head">
        <div className="mu-pane-title-row">
          <AgentLogo harness={harness} size="lg" />
          <h2 className={`mu-pane-title ${hasKeys ? "app-pane-title--keys" : ""}`} title={title}>
            {title}
          </h2>
          <div className="mu-pane-actions">
            <ShellButton session={session} />
            <Button
              icon="changes"
              title="Changes (d)"
              aria-pressed={mode === "changes"}
              onClick={changes}
            >
              Changes
            </Button>
            <FullScreenButton session={session} />
            {menu.length ? (
              <IconButton
                icon="more"
                label="More"
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen(!menuOpen)}
              />
            ) : null}
          </div>
          {menuOpen ? (
            <SessionMenu label={title} items={menu} onClose={() => setMenuOpen(false)} />
          ) : null}
        </div>
        <PaneMeta session={session} />
      </div>
      {mode === "changes" && workspace ? (
        <Suspense fallback={null}>
          <ChangesView key={workspace} workspace={workspace} />
        </Suspense>
      ) : (
        <>
          <NeedsYouBanner session={session} />
          <div className="app-pane-terminal session-terminal">
            <Panels session={session} />
          </div>
          {workspace ? <ThisTurn key={workspace} workspace={workspace} /> : null}
        </>
      )}
    </section>
  );
}
