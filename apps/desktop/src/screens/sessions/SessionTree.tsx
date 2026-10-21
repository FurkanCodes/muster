// The tree itself (design system: SessionTree): the lines drawn, each
// project's under it and each worktree's sessions under that, in
// `mu-tree-children` groups, as one ARIA tree whose selection the window's
// keys move (`aria-activedescendant`).

import { type ReactNode, useEffect } from "react";
import { useDaemon } from "../../app/context";
import type { SessionActions } from "../../session/useSessionActions";
import { RenameSession } from "../../session/useSessionActions";
import { standsBeside } from "../../state/selectors";
import {
  BesideLine,
  type LineProps,
  ProjectLine,
  rowId,
  SessionLine,
  WorktreeLine,
} from "./TreeRows";
import type { TreeRow } from "./tree";

interface Node {
  row: TreeRow;
  children: Node[];
}

/** The lines drawn, nested: projects, their workspaces, their sessions. */
function nested(rows: readonly TreeRow[]): Node[] {
  const projects: Node[] = [];
  let workspace: Node | null = null;
  for (const row of rows) {
    const node = { row, children: [] };
    if (row.kind === "project") {
      projects.push(node);
      workspace = null;
    } else if (row.kind === "workspace") {
      projects.at(-1)?.children.push(node);
      workspace = node;
    } else workspace?.children.push(node);
  }
  return projects;
}

function SessionOrBeside({
  props,
  panelKeys,
}: {
  props: LineProps;
  panelKeys: Readonly<Record<string, string>>;
}) {
  const harness = useDaemon((state) => state.sessions[props.row.id]?.harness ?? "");
  if (!standsBeside({ harness })) return <SessionLine {...props} />;
  return <BesideLine {...props} panelKey={panelKeys[harness] ?? null} />;
}

export function SessionTree({
  rows,
  folded,
  selected,
  resting,
  renaming,
  panelKeys,
  onPick,
  onFold,
}: {
  rows: readonly TreeRow[];
  folded: ReadonlySet<string>;
  selected: string | undefined;
  resting: boolean;
  renaming: SessionActions | null;
  /** Each plugin's panel's key, by its harness, as its line ends. */
  panelKeys: Readonly<Record<string, string>>;
  onPick: (id: string) => void;
  onFold: (id: string) => void;
}) {
  // Scrolled into view as it is selected.
  useEffect(() => {
    if (selected) document.getElementById(rowId(selected))?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);

  const draw = (node: Node, level: number): ReactNode => {
    const { row } = node;
    const props: LineProps = {
      row,
      level,
      selected: row.id === selected,
      resting,
      folded: folded.has(row.id),
      onPick,
      onFold,
    };
    let line: ReactNode;
    if (row.kind === "project") line = <ProjectLine {...props} />;
    else if (row.kind === "workspace") line = <WorktreeLine {...props} />;
    else if (renaming?.renaming === row.id) line = <RenameSession actions={renaming} />;
    else line = <SessionOrBeside props={props} panelKeys={panelKeys} />;
    return (
      <div key={row.id} className="sessions-node" role="none">
        {line}
        {node.children.length ? (
          // biome-ignore lint/a11y/useSemanticElements: an ARIA tree's group of lines, not a form's fieldset.
          <div className="mu-tree-children" role="group">
            {node.children.map((child) => draw(child, level + 1))}
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <div
      className="mu-tree sessions-tree"
      role="tree"
      tabIndex={0}
      aria-label="Projects, worktrees and sessions"
      aria-activedescendant={selected ? rowId(selected) : undefined}
    >
      {nested(rows).map((node) => draw(node, 1))}
    </div>
  );
}
