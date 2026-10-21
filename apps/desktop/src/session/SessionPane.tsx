// The detail pane beside a list: which session, where it runs, and its live
// terminal (design system: PaneHeader, Terminal). The queue branch adds the
// needs-you banner and This turn; the changes branch fills ChangesView.

import { lazy, Suspense } from "react";
import { useActions, useDaemon, useUi } from "../app/context";
import { useKeys } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import { agentOf } from "../lib/agents";
import { spend } from "../lib/format";
import { branchOf, projectColour } from "../state/selectors";
import { Terminal } from "../terminal/Terminal";
import { Button } from "../ui/controls";
import { AgentLogo, Branch, PullRequest } from "../ui/status";

const ChangesView = lazy(() => import("./changes/ChangesView"));

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
      {pr ? <PullRequest pr={pr} /> : null}
    </div>
  );
}

export function SessionPane({ session }: { session: SessionId }) {
  const title = useDaemon((state) => state.sessions[session]?.title ?? "");
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const workspace = useDaemon((state) => state.sessions[session]?.workspace ?? null);
  const hasKeys = useUi((state) => state.keysIn === "terminal" && state.terminal === session);
  const mode = useUi((state) => state.pane);
  const actions = useActions();
  const changes = () => {
    actions.keysTo("list");
    actions.showPane(mode === "changes" ? "terminal" : "changes");
  };
  useKeys({ d: changes });
  return (
    <section className="mu-pane app-pane" aria-label={title}>
      <div className="mu-pane-head">
        <div className="mu-pane-title-row">
          <AgentLogo harness={harness} size="lg" />
          <h2 className={`mu-pane-title ${hasKeys ? "app-pane-title--keys" : ""}`} title={title}>
            {title}
          </h2>
          <div className="mu-pane-actions">
            <Button title="Back to the terminal (c)" onClick={() => actions.showPane("terminal")}>
              Terminal
            </Button>
            <Button title="See changes (d)" aria-pressed={mode === "changes"} onClick={changes}>
              Changes
            </Button>
          </div>
        </div>
        <PaneMeta session={session} />
      </div>
      {mode === "changes" && workspace ? (
        <Suspense fallback={null}>
          <ChangesView key={workspace} workspace={workspace} />
        </Suspense>
      ) : (
        <div className="app-pane-terminal">
          <Terminal session={session} />
        </div>
      )}
    </section>
  );
}
