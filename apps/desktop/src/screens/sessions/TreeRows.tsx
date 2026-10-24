// The lines of the All list (design system: SessionTree): a project's, with
// its swatch, which opens its page, where work starts; a session's, as a two-line
// row (or one, where the person says); a worktree's, with its branch and
// what its work comes to, while no session is in it; and a shell's or a
// plugin's panel's on one line, where its workspace has no chat for it to
// stand by. Folded,
// a project's line says how the most urgent of what it hides stands, and how
// many it hides.

import type { MouseEvent, ReactNode } from "react";
import { useNow } from "../../app/clock";
import { useDaemon } from "../../app/context";
import type { Session } from "../../ipc/generated/Session";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { branchOf, projectColour, shownHarness } from "../../state/selectors";
import { ListItem, ListItemEnd, ListItemMeta, ListItemText, ListItemTitle } from "../../ui/List";
import {
  AgentLogo,
  BranchName,
  DiffStat,
  NewsDot,
  PullRequest,
  StatusMark,
  Swatch,
} from "../../ui/status";
import { mostUrgent, sessionsOn, type TreeRow } from "./tree";

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
  /** For a project's line, whether it is folded. */
  folded: boolean;
  onPick: (id: string) => void;
  onFold: (id: string) => void;
}

function Age({ since, unseen = false }: { since: number; unseen?: boolean }) {
  const now = useNow();
  return (
    <ListItemEnd>
      {ago(now - since)}
      {unseen ? <NewsDot /> : null}
    </ListItemEnd>
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
  if (hidden === 0) return <ListItemEnd>no sessions</ListItemEnd>;
  return (
    <ListItemEnd>
      {kind ? <StatusMark kind={kind} /> : null}
      {news ? <NewsDot /> : null}
      {hidden} folded
    </ListItemEnd>
  );
}

function Fold({ row, folded, onFold }: Pick<LineProps, "row" | "folded" | "onFold">) {
  const fold = (event: MouseEvent) => {
    event.stopPropagation();
    onFold(row.id);
  };
  return (
    // The list folds by its keys (← → e); this is the pointer's way.
    <span className={`mu-tree-fold${folded ? " is-folded" : ""}`} aria-hidden="true" onClick={fold}>
      {folded ? "▸" : "▾"}
    </span>
  );
}

function Line({
  row,
  level,
  selected,
  expanded,
  density,
  className,
  label,
  onPick,
  children,
}: Pick<LineProps, "row" | "level" | "selected" | "onPick"> & {
  expanded?: boolean;
  density: "comfortable" | "compact";
  className?: string;
  label: string;
  children: ReactNode;
}) {
  return (
    <ListItem
      id={rowId(row.id)}
      selected={selected}
      label={label}
      level={level}
      expanded={expanded}
      density={density}
      className={className}
      onSelect={() => onPick(row.id)}
    >
      {children}
    </ListItem>
  );
}

export function ProjectLine(props: LineProps) {
  const { row, folded } = props;
  const name = useDaemon((state) => state.projects[row.id]?.name ?? "");
  const root = useDaemon((state) => state.projects[row.id]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, row.id));
  return (
    <Line {...props} expanded={!folded} density="compact" className="sessions-project" label={name}>
      <Fold {...props} />
      <Swatch colour={colour} />
      <span className="mu-tree-project mu-cut" title={homely(root)}>
        {name}
      </span>
      {folded ? <Standing row={row} /> : null}
    </Line>
  );
}

/** A worktree no session is in: its branch, and what its work there comes to. */
export function WorktreeLine(props: LineProps) {
  const { row } = props;
  const branch = useDaemon((state) => {
    const workspace = state.workspaces[row.id];
    return workspace ? branchOf(workspace) : "";
  });
  const inGit = useDaemon((state) => Boolean(state.workspaces[row.id]?.kind.branch));
  const added = useDaemon((state) => state.workspaces[row.id]?.changes?.added ?? 0);
  const removed = useDaemon((state) => state.workspaces[row.id]?.changes?.removed ?? 0);
  const pr = useDaemon((state) => state.workspaces[row.id]?.pull_request ?? null);
  return (
    <Line
      {...props}
      density="compact"
      className="sessions-worktree"
      label={inGit ? branch : "not a git repository"}
    >
      {inGit ? (
        <BranchName className="mu-cut">{branch}</BranchName>
      ) : (
        <span className="mu-quiet mu-cut">not a git repository</span>
      )}
      <ListItemEnd>
        {added || removed ? <DiffStat added={added} removed={removed} /> : "no sessions"}
        {pr ? <PullRequest pr={pr} words={false} /> : null}
      </ListItemEnd>
    </Line>
  );
}

function ChatText({ session, lines }: { session: Session; lines: "one" | "two" }) {
  const asking = session.status.state === "needs_you";
  const what = doing(session);
  const name = (
    <ListItemTitle className={asking ? "sessions-asking" : undefined} title={session.title}>
      {session.title}
    </ListItemTitle>
  );
  const words = <ListItemMeta title={what}>{what}</ListItemMeta>;
  if (lines === "one")
    return (
      <>
        <span className="sessions-one-name">{name}</span>
        <span className="sessions-one-doing">{words}</span>
      </>
    );
  return (
    <ListItemText>
      {name}
      {words}
    </ListItemText>
  );
}

export function SessionLine(props: LineProps) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  const lines = useDaemon((state) => state.preferences?.session_lines ?? "two");
  if (!session) return null;
  const unseen = session.unseen && session.status.state !== "needs_you";
  return (
    <Line {...props} density="comfortable" label={`${session.title}, ${doing(session)}`}>
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={shownHarness(session)} />
      <ChatText session={session} lines={lines} />
      <Age since={session.status_since} unseen={unseen} />
    </Line>
  );
}

/** A shell's or a plugin's panel's line, where no chat in its workspace has it beside (`view::sessions::shell_line`). */
export function BesideLine(props: LineProps & { panelKey: string | null }) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  // A plugin's panel goes by its plugin's name, as the PluginPanel board writes it.
  const plugin = useDaemon((state) => {
    const harness = state.sessions[props.row.id]?.harness ?? "";
    const id = harness.startsWith("plugin:") ? harness.slice(7).split("/")[0] : null;
    return id ? (state.plugins.find((one) => one.id === id)?.details?.name ?? null) : null;
  });
  if (!session) return null;
  const shell = session.harness === "shell";
  const name = shell ? session.title : (plugin ?? session.title);
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
      density="compact"
      className="mu-tree-shell"
      label={`${shell ? "Shell" : "Panel"}: ${name}${ended ? ", ended" : ""}`}
    >
      <span className={`mu-cut ${ended || waiting ? "" : "sessions-running"}`}>
        {shell ? "❯" : "◇"} {name}
      </span>
      {end === null ? null : <ListItemEnd>{end}</ListItemEnd>}
    </Line>
  );
}
