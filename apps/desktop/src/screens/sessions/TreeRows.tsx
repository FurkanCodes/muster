// The lines of the All list (design system: SessionTree): a project's, with
// its swatch, which opens its page, where work starts; a session's, as a two-line
// row (or one, where the person says); a worktree's, with its branch and
// what its work comes to, while no session is in it; and a shell's or a
// plugin's panel's on one line, where its workspace has no chat for it to
// stand by. Folded,
// a project's line says how the most urgent of what it hides stands, and how
// many it hides.

import type { PointerEvent, ReactNode } from "react";
import { useNow } from "../../app/clock";
import { useApp, useDaemon, useUi } from "../../app/context";
import { addProjectIn } from "../../flows/actions";
import { offered } from "../../flows/agents";
import { startIn } from "../../flows/start";
import type { AddedAgent } from "../../ipc/generated/AddedAgent";
import type { Session } from "../../ipc/generated/Session";
import { useKeymap } from "../../keys/useKeymap";
import { ago } from "../../lib/format";
import { doing, type StatusKind, statusKind } from "../../lib/status";
import { pressToCarry } from "../../panels/carry";
import { terminalIn } from "../../spaces/spaces";
import { branchOf, projectColour, shownHarness, standsBeside } from "../../state/selectors";
import { IconButton } from "../../ui/controls";
import { Icon } from "../../ui/Icon";
import { ListItem, ListItemEnd, ListItemMeta, ListItemText, ListItemTitle } from "../../ui/List";
import { ActionMenu, keysOf, type MenuItem } from "../../ui/Menu";
import { AgentLogo, BranchName, DiffStat, NewsDot, PullRequest, StatusMark } from "../../ui/status";
import { mostUrgent, sessionsOn, type TreeRow } from "./tree";

const NO_AGENTS: AddedAgent[] = [];

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

/** A space's chevron: down while what it holds shows, turned right while folded. */
function Fold({ folded }: Pick<LineProps, "folded">) {
  return (
    <span className={`mu-tree-fold${folded ? " is-folded" : ""}`} aria-hidden="true">
      <Icon name="chevron" size={12} />
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
  onCarry,
  children,
}: Pick<LineProps, "row" | "level" | "selected" | "onPick"> & {
  expanded?: boolean;
  density: "comfortable" | "compact";
  className?: string;
  label: string;
  onCarry?: (event: PointerEvent<HTMLElement>) => void;
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
      onCarry={onCarry}
    >
      {children}
    </ListItem>
  );
}

export function ProjectLine(props: LineProps) {
  const { row, folded, onFold } = props;
  const name = useDaemon((state) => state.projects[row.id]?.name ?? "");
  const root = useDaemon((state) => state.projects[row.id]?.root ?? "");
  const colour = useDaemon((state) => projectColour(state, row.id));
  return (
    <>
      <Line
        {...props}
        expanded={!folded}
        density="compact"
        className={`sessions-project sessions-project--p${colour}`}
        label={name}
        // Let go on another project's lines, it goes there (projectOrder).
        onCarry={(event) => pressToCarry(event, { kind: "project", id: row.id })}
      >
        {/* biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useKeyWithClickEvents: the list folds it by its keys (← → e); the chevron is the pointer's way, and the name opens its page. */}
        <span
          className="sessions-project-fold"
          onClick={(event) => {
            event.stopPropagation();
            onFold(row.id);
          }}
        >
          <Fold folded={folded} />
        </span>
        <span className="mu-tree-project mu-cut" title={homely(root)}>
          {name}
        </span>
        {folded ? <Standing row={row} /> : null}
      </Line>
      <ProjectActions project={row.id} name={name} />
    </>
  );
}

/** A space's heading (`ARC-33`): its chevron, turned as it folds, and its name; what it holds hangs from a line under the chevron. */
export function SpaceLine(props: LineProps) {
  const { row, folded, onFold } = props;
  const name = useDaemon((state) => state.spaces.find((space) => space.id === row.id)?.name ?? "");
  return (
    <>
      <Line {...props} expanded={!folded} density="compact" className="sessions-space" label={name}>
        {/* biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useKeyWithClickEvents: the list folds it by its keys (← → e); a click on the chevron or the name is the pointer's way, and selects it as any click does. */}
        <span className="sessions-space-head" onClick={() => onFold(row.id)}>
          <Fold folded={folded} />
          <span className="sessions-space-name mu-cut">{name}</span>
        </span>
        {folded ? <Standing row={row} /> : null}
      </Line>
      <SpaceActions space={row.id} name={name} />
    </>
  );
}

/** What a space's heading offers under the pointer: a terminal in it, and what can be added to it. */
function SpaceActions({ space, name }: { space: string; name: string }) {
  const app = useApp();
  const map = useKeymap();
  // What is added goes in the selected line's space: this one's.
  const here = (then: () => void) => () => {
    app.actions.select("all", space);
    then();
  };
  return (
    <span className="sessions-row-actions">
      <IconButton
        icon="terminal"
        size="sm"
        label={`Terminal in ${name}`}
        onClick={() => void terminalIn(app, space)}
      />
      <ActionMenu
        label={`Add to ${name}`}
        items={[
          {
            ...keysOf(map, "add_project"),
            label: "Folder or project…",
            does: here(() => addProjectIn(app)),
          },
          {
            ...keysOf(map, "clone_project"),
            label: "Clone a GitHub repository…",
            does: here(() => app.actions.openFlow({ kind: "clone-project" })),
          },
          { key: "", label: "Terminal", does: () => void terminalIn(app, space) },
        ]}
        trigger={<IconButton icon="plus" size="sm" label={`Add to ${name}`} />}
      />
    </span>
  );
}

/** What a project's line offers under the pointer: a shell in its checkout, and an agent started in it. */
function ProjectActions({ project, name }: { project: string; name: string }) {
  const app = useApp();
  const added = useDaemon((state) => state.preferences?.agents ?? NO_AGENTS);
  const last = useUi((state) => state.lastAgent);
  const plain = useDaemon((state) =>
    Object.values(state.workspaces).some(
      (workspace) =>
        workspace.project === project &&
        workspace.kind.kind === "checkout" &&
        workspace.kind.branch === "",
    ),
  );
  const agents = offered(added, last, "session").filter((agent) => agent.id !== "shell");
  const items: MenuItem[] = [
    ...agents.map((agent) => ({
      key: "",
      label: agent.name,
      does: () => void startIn(app, project, agent.id),
    })),
    ...(plain
      ? []
      : [
          {
            key: "",
            label: "New worktree…",
            does: () => app.actions.openFlow({ kind: "new-worktree", project }),
          },
        ]),
  ];
  return (
    <span className="sessions-row-actions">
      <IconButton
        icon="terminal"
        size="sm"
        label={`Shell in ${name}`}
        onClick={() => void startIn(app, project, "shell")}
      />
      <ActionMenu
        label={`New in ${name}`}
        items={items}
        trigger={<IconButton icon="plus" size="sm" label={`New in ${name}`} />}
      />
    </span>
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
  // Agents at work on its branch at once edit the same files: said, so it
  // stands out. One finished, or ended, is at work no more.
  const agents = useDaemon(
    (state) =>
      Object.values(state.sessions).filter(
        (session) =>
          session.workspace === row.id &&
          !standsBeside(session) &&
          ["running", "needs_you", "fresh"].includes(session.status.state),
      ).length,
  );
  const end: ReactNode =
    added || removed ? (
      <DiffStat added={added} removed={removed} />
    ) : row.place ? null : (
      "no sessions"
    );
  return (
    <Line
      {...props}
      density="compact"
      className={row.place ? "sessions-worktree sessions-place" : "sessions-worktree"}
      label={inGit ? branch : "not a git repository"}
    >
      {inGit ? (
        <BranchName className="mu-cut">{branch}</BranchName>
      ) : (
        <span className="mu-quiet mu-cut">not a git repository</span>
      )}
      <ListItemEnd>
        {row.place && agents > 1 ? (
          <span
            className="sessions-place-agents"
            title="They work on the same branch, in the same files"
          >
            {agents} agents
          </span>
        ) : null}
        {/* A label is its branch: what its work comes to is the pane's. */}
        {row.place ? null : end}
        {pr && !row.place ? <PullRequest pr={pr} words={false} /> : null}
      </ListItemEnd>
    </Line>
  );
}

/**
 * A chat's name over what it does; its branch, checkout or worktree, in the
 * line's tip. Where its project works in more than its checkout, its place's
 * label names the branch over it.
 */
function ChatText({ session, lines }: { session: Session; lines: "one" | "two" }) {
  const asking = session.status.state === "needs_you";
  const branch = useDaemon((state) => {
    const workspace = session.workspace ? state.workspaces[session.workspace] : undefined;
    return workspace && workspace.kind.kind !== "container" ? branchOf(workspace) : "";
  });
  const said = doing(session);
  const tip = branch ? `${said} · ⎇ ${branch}` : said;
  return <LineText name={session.title} what={said} tip={tip} asking={asking} lines={lines} />;
}

/** A session's name over what it does, or both on one line, as the person's settings have it. */
function LineText({
  name: title,
  what,
  tip,
  asking = false,
  lines,
}: {
  name: string;
  what: ReactNode;
  /** What the line's second part says in full, where it is cut. */
  tip?: string;
  asking?: boolean;
  lines: "one" | "two";
}) {
  const name = (
    <ListItemTitle className={asking ? "sessions-asking" : undefined} title={title}>
      {title}
    </ListItemTitle>
  );
  const words = (
    <ListItemMeta title={tip ?? (typeof what === "string" ? what : undefined)}>{what}</ListItemMeta>
  );
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
    <Line
      {...props}
      density="comfortable"
      label={`${session.title}, ${doing(session)}`}
      // Let go on the pane, it is placed there in a panel of its own (Panels).
      onCarry={(event) => pressToCarry(event, { kind: "session", id: session.id })}
    >
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={shownHarness(session)} />
      <ChatText session={session} lines={lines} />
      <Age since={session.status_since} unseen={unseen} />
    </Line>
  );
}

/**
 * A shell's line, drawn as a chat's is: its mark and how it stands, the
 * program in front in it over what it is doing, and for how long; carried
 * to the pane as a chat's line is (`view::sessions::shell_line`).
 */
export function ShellLine(props: LineProps) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  const lines = useDaemon((state) => state.preferences?.session_lines ?? "two");
  if (!session) return null;
  const ended = session.ended_at !== null || session.status.state === "exited";
  const waiting = SHELLS.has(session.title);
  // A program in front of a shell may be busy or waiting on the person, and
  // nothing says which: the shell is the person's, not an agent, so its mark
  // stays still, never spinning as if at work.
  const kind: StatusKind = ended ? statusKind(session) : "fresh";
  const what = ended ? "ended" : waiting ? "at its prompt" : "open";
  return (
    <Line
      {...props}
      density="comfortable"
      label={`Shell: ${session.title}${ended ? ", ended" : ""}`}
      onCarry={(event) => pressToCarry(event, { kind: "session", id: session.id })}
    >
      <StatusMark kind={kind} />
      <AgentLogo harness="shell" />
      <LineText name={session.title} what={what} lines={lines} />
      <Age since={session.status_since} />
    </Line>
  );
}

/** A plugin's panel's line, where no chat in its workspace has it beside (`view::sessions::shell_line`). */
export function BesideLine(props: LineProps & { panelKey: string | null }) {
  const session = useDaemon((state) => state.sessions[props.row.id]);
  // A plugin's panel goes by its plugin's name, as the PluginPanel board writes it.
  const plugin = useDaemon((state) => {
    const harness = state.sessions[props.row.id]?.harness ?? "";
    const id = harness.startsWith("plugin:") ? harness.slice(7).split("/")[0] : null;
    return id ? (state.plugins.find((one) => one.id === id)?.details?.name ?? null) : null;
  });
  if (!session) return null;
  const name = plugin ?? session.title;
  const ended = session.ended_at !== null || session.status.state === "exited";
  const end: ReactNode = ended ? "ended" : props.panelKey;
  return (
    <Line
      {...props}
      density="compact"
      className="mu-tree-shell"
      label={`Panel: ${name}${ended ? ", ended" : ""}`}
    >
      <span className={`mu-cut ${ended ? "" : "sessions-running"}`}>◇ {name}</span>
      {end === null ? null : <ListItemEnd>{end}</ListItemEnd>}
    </Line>
  );
}
