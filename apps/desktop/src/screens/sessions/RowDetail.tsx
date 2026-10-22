// What is beside the list while a worktree's line, one no session is in,
// is selected: what it is and where, and what its keys do there. A
// session's line shows the session itself (SessionPane), a project's its
// page (ProjectPage).

import { useDaemon } from "../../app/context";
import { Page, PageHeader, PageMeta, PageTitle } from "../../app/Page";
import { branchOf, projectColour } from "../../state/selectors";
import { EmptyState, EmptyStateText } from "../../ui/EmptyState";
import { DiffStat, PullRequest } from "../../ui/status";
import type { TreeRow } from "./tree";

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
      <PageTitle className="mu-branch">⎇ {branch}</PageTitle>
      <PageMeta>
        <span className="mu-where">
          in <span className={`mu-p${colour}`}>{project}</span>
        </span>
        {added === null ? null : <DiffStat added={added} removed={removed} files={files} />}
        {pr ? <PullRequest pr={pr} /> : null}
      </PageMeta>
    </>
  );
}

export function RowDetail({ row, shellKey }: { row: TreeRow; shellKey: string }) {
  return (
    <Page label="Selected line" className="sessions-detail">
      <PageHeader>
        <WorktreeHead id={row.id} />
      </PageHeader>
      <EmptyState className="sessions-detail-empty">
        <EmptyStateText>
          No session works here yet: ⏎ starts one. {shellKey} opens a shell here.
        </EmptyStateText>
      </EmptyState>
    </Page>
  );
}
