import type { WorkspaceId } from "../../ipc/generated/WorkspaceId";

/** A workspace's changes, diffs and turns: built on feature/desktop-changes. */
export default function ChangesView({ workspace }: { workspace: WorkspaceId }) {
  return (
    <div className="mu-empty">
      <p className="mu-empty-text">
        Changes of <span className="mu-mono">{workspace}</span>, on feature/desktop-changes.
      </p>
    </div>
  );
}
