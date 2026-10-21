// One session of the queue, as the board draws it (design system:
// QueueItem, QueueRow): a session asking something is an item with its
// question; one that finished unseen a two-line row with its branch, what
// its work comes to and its pull request; one at work a row saying what it
// is doing, on one line.

import { type ReactNode, useEffect } from "react";
import { useNow } from "../../app/clock";
import { useApp, useDaemon } from "../../app/context";
import type { Session } from "../../ipc/generated/Session";
import type { SessionId } from "../../ipc/generated/SessionId";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { questionOf } from "../../session/question";
import { branchOf, projectColour } from "../../state/selectors";
import { Hint } from "../../ui/controls";
import { AgentLogo, Branch, DiffStat, NewsDot, PullRequest, StatusMark } from "../../ui/status";
import { changedInSight, stamp } from "./history";
import { sectionOf } from "./queue";

/** The option's id in the list, for `aria-activedescendant`. */
export function optionId(session: SessionId): string {
  return `queue-${session}`;
}

const FLASHES = { waiting: "waiting", error: "waiting", done: "done", ended: "done" } as const;

function Age({ since, unseen = false }: { since: number; unseen?: boolean }) {
  const now = useNow();
  return (
    <span className="mu-age">
      {ago(now - since)}
      {unseen ? <NewsDot /> : null}
    </span>
  );
}

/** Where the session runs: its branch, its project, and what the project is. */
function useWhere(session: Session) {
  return {
    workspace: useDaemon((state) => state.workspaces[session.workspace]),
    project: useDaemon((state) => {
      const project = state.workspaces[session.workspace]?.project;
      return project ? state.projects[project]?.name : undefined;
    }),
    colour: useDaemon((state) => {
      const project = state.workspaces[session.workspace]?.project;
      return project ? projectColour(state, project) : 1;
    }),
  };
}

function Asking({ session, keys }: { session: Session; keys: boolean }) {
  const { workspace, project, colour } = useWhere(session);
  const screen = useDaemon((state) => state.derived.questions[session.id]);
  const [ask, ...rest] = questionOf(session, screen);
  return (
    <>
      <span className="mu-item-head">
        <StatusMark kind={statusKind(session)} />
        <AgentLogo harness={session.harness} />
        <span className="mu-item-name" title={session.title}>
          {session.title}
        </span>
        <Age since={session.status_since} />
      </span>
      <span className="mu-item-under mu-cut">
        <Branch
          branch={workspace ? branchOf(workspace) : "no branch"}
          project={project}
          colour={colour}
        />
      </span>
      <span className="mu-question">
        <span className="mu-question-ask">{ask}</span>
        {rest.map((line) => `\n${line}`).join("")}
      </span>
      {keys ? (
        <span className="mu-item-under mu-hints">
          <Hint keys="⏎" does="answer" />
          <Hint keys="d" does="see changes" />
        </span>
      ) : null}
    </>
  );
}

function Finished({ session }: { session: Session }) {
  const { workspace } = useWhere(session);
  const changes = workspace?.changes;
  return (
    <>
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={session.harness} />
      <span className="mu-row-text">
        <span className="mu-cut" title={session.title}>
          {session.title}
        </span>
        <span className="mu-row-line mu-meta">
          <span className="mu-branch mu-cut">
            ⎇ {workspace ? branchOf(workspace) : "no branch"}
          </span>
          {changes && (changes.added || changes.removed) ? (
            <DiffStat added={changes.added} removed={changes.removed} />
          ) : (
            <span>no changes</span>
          )}
          {workspace?.pull_request ? <PullRequest pr={workspace.pull_request} /> : null}
        </span>
      </span>
      <Age since={session.status_since} unseen={session.unseen} />
    </>
  );
}

function Working({ session }: { session: Session }) {
  const what = doing(session);
  return (
    <>
      <StatusMark kind="working" />
      <AgentLogo harness={session.harness} />
      <span className="mu-cut queue-name" title={session.title}>
        {session.title}
      </span>
      <span className="mu-meta mu-cut queue-doing" title={what}>
        {what}
      </span>
      <Age since={session.status_since} />
    </>
  );
}

export function QueueEntry({
  id,
  selected,
  resting,
  onPick,
}: {
  id: SessionId;
  selected: boolean;
  /** Selected while the keys are elsewhere (the terminal, a dialog). */
  resting: boolean;
  onPick: (id: SessionId) => void;
}) {
  const app = useApp();
  const session = useDaemon((state) => state.sessions[id]);
  // Scrolled into view as it is selected, by key or by `l`.
  useEffect(() => {
    if (selected) document.getElementById(optionId(id))?.scrollIntoView?.({ block: "nearest" });
  }, [selected, id]);
  if (!session) return null;
  const section = sectionOf(session);
  const kind = statusKind(session);
  const tone = kind in FLASHES ? FLASHES[kind as keyof typeof FLASHES] : null;
  const flash = tone && changedInSight(app.daemon, session) ? `mu-flash mu-flash--${tone}` : "";
  let look: string;
  let content: ReactNode;
  if (section === "asking") {
    look = "mu-item";
    content = <Asking session={session} keys={selected && !resting} />;
  } else if (section === "working") {
    look = "mu-row";
    content = <Working session={session} />;
  } else {
    look = "mu-row mu-row--two";
    content = <Finished session={session} />;
  }
  return (
    <button
      // A new status flashes once: the row is drawn afresh for it.
      key={flash ? stamp(session) : "still"}
      id={optionId(id)}
      type="button"
      role="option"
      tabIndex={-1}
      aria-selected={selected}
      aria-label={session.title}
      className={`${look} ${resting ? "is-resting" : ""} ${flash}`}
      onClick={() => onPick(id)}
    >
      {content}
    </button>
  );
}
