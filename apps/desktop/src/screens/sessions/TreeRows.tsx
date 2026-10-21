// The lines of the All sessions tree, as the AllSessions board draws them
// (design system: SessionTree): a project's, with its swatch and folder; a
// worktree's, with its branch, what its work comes to, whether it meets
// other work and its pull request; a session's, as a two-line row (or one,
// where the person says); and a shell's or a plugin's panel's, on one line
// beside the chat it serves. Folded, a project's or a worktree's line says
// how the most urgent of what it hides stands, and how many it hides.

import type { MouseEvent, ReactNode } from "react";
import { useNow } from "../../app/clock";
import { useDaemon } from "../../app/context";
import type { Session } from "../../ipc/generated/Session";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { branchOf, projectColour } from "../../state/selectors";
import { AgentLogo, DiffStat, NewsDot, PullRequest, StatusMark } from "../../ui/status";
import { foldable, mostUrgent, sessionsOn, type TreeRow, underItsChat } from "./tree";

/** The row's id in the tree, for `aria-activedescendant`. */
export function rowId(id: string): string {
  return `tree-${id}`;
}

/** The names a shell goes by while it waits at its prompt (`view::sessions::SHELLS`). */
const SHELLS = new Set([
  "zsh",
  "bash",
  "fish",
  "sh",
  "dash",
  "ksh",
  "tcsh",
  "csh",
  "nu",
  "elvish",
  "xonsh",
  "pwsh",
]);

export interface LineProps {
  row: TreeRow;
  level: number;
  selected: boolean;
  /** Selected while the keys are elsewhere: the terminal or a dialog. */
  resting: boolean;
  /** For a project's or a worktree's line, whether it is folded. */
  folded: boolean;
  onPick: (id: string) => void;
  onFold: (id: string) => void;
}

function Age({ since, unseen = false }: { since: number; unseen?: boolean }) {
  const now = useNow();
  return (
    <span className="mu-age">
      {ago(now - since)}
      {unseen ? <NewsDot /> : null}
    </span>
  );
}

/** A home folder written `~`, as the board writes a project's folder. */
function homely(path: string): string {
  return path.replace(/^\/(Users|home)\/[^/]+/, "~");
}

/** Folded: the most urgent mark of what it hides and how many; unfolded, nothing. */
function Standing({ row }: { row: TreeRow }) {
  const kind = useDaemon((state) => {
    const urgent = mostUrgent(state, row);
    return urgent ? statusKind(urgent) : null;
  });
  const news = useDaemon((state) => {
    const urgent = mostUrgent(state, row);
    return Boolean(urgent?.unseen && urgent.status.state !== "needs_you");
  });
  const hidden = useDaemon((state) => sessionsOn(state, row).length);
  if (hidden === 0) return <span className="mu-tree-end">no sessions</span>;
  return (
    <span className="mu-tree-end">
      {kind ? <StatusMark kind={kind} /> : null}
      {news ? <NewsDot /> : null}
      {hidden} folded
    </span>
  );
}

function Fold({ row, folded, onFold }: Pick<LineProps, "row" | "folded" | "onFold">) {
  const can = useDaemon((state) => foldable(state, row));
  const fold = (event: MouseEvent) => {
    event.stopPropagation();
    onFold(row.id);
  };
  return (
    // The tree folds by its keys (← → e); this is the pointer's way.
    <span className="mu-tree-fold" aria-hidden="true" onClick={can || folded ? fold : undefined}>
      {folded ? "▸" : can ? "▾" : ""}
    </span>
  );
}

function Line({
  row,
  level,
  selected,
  resting,
  expanded,
  className,
  label,
  onPick,
  children,
}: Pick<LineProps, "row" | "level" | "selected" | "resting" | "onPick"> & {
  expanded?: boolean;
  className: string;
  label: string;
  children: ReactNode;
}) {
  return (
    <button
      id={rowId(row.id)}
      type="button"
      role="treeitem"
      tabIndex={-1}
      aria-level={level}
      aria-selected={selected}
      aria-expanded={expanded}
      aria-label={label}
      className={`${className} ${selected && resting ? "is-resting" : ""}`}
      onClick={() => onPick(row.id)}
    >
      {children}
    </button>
  );
}

export function ProjectLine(props: LineProps) {
  const { row, folded } = props;
  const name = useDaemon((state) => state.projects[row.id]?.name ?? "");
  const root = useDaemon((state) => state.projects[row.id]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, row.id));
  return (
    <Line {...props} expanded={!folded} className="mu-tree-row sessions-project" label={name}>
      <Fold {...props} />
      <span className={`mu-swatch mu-p${colour}`} />
      <span className={`mu-tree-project mu-p${colour}`}>{name}</span>
      {folded ? (
        <Standing row={row} />
      ) : (
        <span className="mu-tree-end mu-mono mu-cut">{homely(root)}</span>
      )}
    </Line>
  );
}

function WorktreeEnd({ id }: { id: string }) {
  const added = useDaemon((state) => state.workspaces[id]?.changes?.added ?? 0);
  const removed = useDaemon((state) => state.workspaces[id]?.changes?.removed ?? 0);
  const clash = useDaemon((state) => {
    const clashes = state.workspaces[id]?.clashes ?? [];
    if (clashes.length === 0) return null;
    return clashes.some((one) => one.conflicts > 0) ? "conflict" : "overlap";
  });
  const pr = useDaemon((state) => state.workspaces[id]?.pull_request ?? null);
  if (!added && !removed && !clash && !pr) return null;
  return (
    <span className="mu-tree-end">
      {added || removed ? <DiffStat added={added} removed={removed} /> : null}
      {clash ? <span className={`mu-clash-${clash}`}>{clash}</span> : null}
      {pr ? <PullRequest pr={pr} /> : null}
    </span>
  );
}

export function WorktreeLine(props: LineProps) {
  const { row, folded } = props;
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[row.id];
    return workspace ? branchOf(workspace) : "";
  });
  const inGit = useDaemon((state) => Boolean(state.workspaces[row.id]?.kind.branch));
  const can = useDaemon((state) => foldable(state, row));
  return (
    <Line
      {...props}
      expanded={can ? !folded : undefined}
      className="mu-tree-row"
      label={inGit ? branch : "not a git repository"}
    >
      <Fold {...props} />
      {inGit ? (
        <span className="mu-branch mu-cut">⎇ {branch}</span>
      ) : (
        <span className="mu-quiet mu-cut">not a git repository</span>
      )}
      {folded ? <Standing row={row} /> : <WorktreeEnd id={row.id} />}
    </Line>
  );
}

function ChatText({ session, lines }: { session: Session; lines: "one" | "two" }) {
  const asking = session.status.state === "needs_you";
  const what = doing(session);
  const name = (
    <span className={`mu-cut ${asking ? "sessions-asking" : ""}`} title={session.title}>
      {session.title}
    </span>
  );
  const words = (
    <span className={`mu-meta mu-cut ${asking ? "sessions-waiting" : ""}`} title={what}>
      {what}
    </span>
  );
  if (lines === "one")
    return (
      <>
        <span className="sessions-one-name">{name}</span>
        <span className="sessions-one-doing">{words}</span>
      </>
    );
  return (
    <span className="mu-row-text">
      {name}
      {words}
    </span>
  );
}

export function SessionLine(props: LineProps) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  const lines = useDaemon((state) => state.preferences?.session_lines ?? "two");
  if (!session) return null;
  const unseen = session.unseen && session.status.state !== "needs_you";
  return (
    <Line
      {...props}
      className={lines === "two" ? "mu-row mu-row--two" : "mu-row"}
      label={`${session.title}, ${doing(session)}`}
    >
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={session.harness} />
      <ChatText session={session} lines={lines} />
      <Age since={session.status_since} unseen={unseen} />
    </Line>
  );
}

/** A shell's or a plugin's panel's line, beside the chat it serves (`view::sessions::shell_line`). */
export function BesideLine(props: LineProps & { panelKey: string | null }) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  const under = useDaemon((state) => {
    const stored = state.sessions[props.row.id];
    return stored ? underItsChat(state, stored) : false;
  });
  if (!session) return null;
  const shell = session.harness === "shell";
  const ended = session.ended_at !== null || session.status.state === "exited";
  const waiting = shell && SHELLS.has(session.title);
  let end: ReactNode = null;
  if (ended) end = "ended";
  else if (!shell) end = props.panelKey;
  else if (waiting) end = "at its prompt";
  else end = <Age since={session.status_since} />;
  return (
    <Line
      {...props}
      className="mu-tree-row mu-tree-shell"
      label={`${shell ? "Shell" : "Panel"}: ${session.title}${ended ? ", ended" : ""}`}
    >
      <span className="mu-tree-fold" aria-hidden="true">
        {under ? "└" : ""}
      </span>
      <span className={`mu-cut ${ended || waiting ? "" : "sessions-running"}`}>
        {shell ? "❯" : "◇"} {session.title}
      </span>
      {end === null ? null : <span className="mu-tree-end">{end}</span>}
    </Line>
  );
}
