// The pane beside the tree while a project's or a worktree's line is
// selected: what it is and where, and what its keys do there. A session's
// line shows the session itself (SessionPane).

import { useDaemon } from "../../app/context";
import { branchOf, projectColour } from "../../state/selectors";
import { DiffStat, PullRequest } from "../../ui/status";
import type { TreeRow } from "./tree";

function ProjectDetail({ id }: { id: string }) {
  const name = useDaemon((state) => state.projects[id]?.name ?? "");
  const root = useDaemon((state) => state.projects[id]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, id));
  return (
    <div className="mu-pane-head">
      <div className="mu-pane-title-row">
        <span className={`mu-swatch mu-p${colour}`} />
        <h2 className={`mu-pane-title mu-p${colour}`}>{name}</h2>
      </div>
      <div className="mu-pane-meta">
        <span className="mu-mono">{root}</span>
      </div>
    </div>
  );
}

function WorktreeDetail({ id }: { id: string }) {
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[id];
    return workspace ? branchOf(workspace) : "";
  });
  const path = useDaemon((state) => {
    const kind = state.workspaces[id]?.kind;
    return kind && "path" in kind ? kind.path : "";
  });
  const project = useDaemon((state) => state.projects[state.workspaces[id]?.project ?? ""]?.name);
  const colour = useDaemon((state) => projectColour(state, state.workspaces[id]?.project ?? ""));
  const added = useDaemon((state) => state.workspaces[id]?.changes?.added ?? null);
  const removed = useDaemon((state) => state.workspaces[id]?.changes?.removed ?? 0);
  const files = useDaemon((state) => state.workspaces[id]?.changes?.files ?? 0);
  const pr = useDaemon((state) => state.workspaces[id]?.pull_request ?? null);
  return (
    <div className="mu-pane-head">
      <div className="mu-pane-title-row">
        <h2 className="mu-pane-title mu-branch">⎇ {branch}</h2>
      </div>
      <div className="mu-pane-meta">
        <span className="mu-where">
          in <span className={`mu-p${colour}`}>{project}</span>
        </span>
        <span className="mu-mono mu-cut">{path}</span>
        {added === null ? null : <DiffStat added={added} removed={removed} files={files} />}
        {pr ? <PullRequest pr={pr} /> : null}
      </div>
    </div>
  );
}

export function RowDetail({ row, shellKey }: { row: TreeRow; shellKey: string }) {
  const sessions = useDaemon((state) =>
    row.kind === "workspace"
      ? Object.values(state.sessions).filter((session) => session.workspace === row.id).length
      : null,
  );
  return (
    <section className="mu-pane app-pane sessions-detail" aria-label="Selected line">
      {row.kind === "project" ? <ProjectDetail id={row.id} /> : <WorktreeDetail id={row.id} />}
      <div className="mu-empty sessions-detail-empty">
        <p className="mu-empty-text">
          {row.kind === "workspace" && sessions === 0
            ? "No session works here yet."
            : "Select a session to see its terminal here, live."}{" "}
          {shellKey} opens a shell {row.kind === "project" ? "in its checkout" : "here"}.
        </p>
      </div>
    </section>
  );
}
