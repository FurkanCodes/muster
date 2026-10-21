// What is beside the list while a project's line, or a worktree's with no
// session in it, is selected: what it is and where, and what its keys do
// there. A session's line shows the session itself (SessionPane).

import { useDaemon } from "../../app/context";
import { SidebarAway } from "../../session/SessionPane";
import { branchOf, projectColour } from "../../state/selectors";
import { DiffStat, PullRequest } from "../../ui/status";
import type { TreeRow } from "./tree";

function ProjectHead({ id }: { id: string }) {
  const name = useDaemon((state) => state.projects[id]?.name ?? "");
  const root = useDaemon((state) => state.projects[id]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, id));
  return (
    <>
      <span className={`mu-swatch mu-p${colour}`} />
      <h2 className="app-head-title">{name}</h2>
      <span className="mu-pane-meta mu-mono mu-cut">{root}</span>
    </>
  );
}

function WorktreeHead({ id }: { id: string }) {
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[id];
    return workspace ? branchOf(workspace) : "";
  });
  const project = useDaemon((state) => state.projects[state.workspaces[id]?.project ?? ""]?.name);
  const colour = useDaemon((state) => projectColour(state, state.workspaces[id]?.project ?? ""));
  const added = useDaemon((state) => state.workspaces[id]?.changes?.added ?? null);
  const removed = useDaemon((state) => state.workspaces[id]?.changes?.removed ?? 0);
  const files = useDaemon((state) => state.workspaces[id]?.changes?.files ?? 0);
  const pr = useDaemon((state) => state.workspaces[id]?.pull_request ?? null);
  return (
    <>
      <h2 className="app-head-title mu-branch">⎇ {branch}</h2>
      <span className="mu-pane-meta app-head-where">
        <span className="mu-where">
          in <span className={`mu-p${colour}`}>{project}</span>
        </span>
        {added === null ? null : <DiffStat added={added} removed={removed} files={files} />}
        {pr ? <PullRequest pr={pr} /> : null}
      </span>
    </>
  );
}

export function RowDetail({ row, shellKey }: { row: TreeRow; shellKey: string }) {
  return (
    <section className="mu-pane app-pane sessions-detail" aria-label="Selected line">
      <header className="app-head" data-tauri-drag-region>
        <SidebarAway />
        {row.kind === "project" ? <ProjectHead id={row.id} /> : <WorktreeHead id={row.id} />}
      </header>
      <div className="mu-empty sessions-detail-empty">
        <p className="mu-empty-text">
          {row.kind === "workspace"
            ? `No session works here yet: ⏎ starts one. ${shellKey} opens a shell here.`
            : `Select a session to see its terminal here, live. ${shellKey} opens a shell in its checkout.`}
        </p>
      </div>
    </section>
  );
}
