// One session of the queue, in the sidebar (design system: QueueRow): one
// asking something a two-line row with what it asks under its name, in full
// over its terminal; one that finished unseen a two-line row with its branch,
// what its work comes to and its pull request; one at work a two-line row
// with what it is doing.

import { useEffect } from "react";
import { useNow } from "../../app/clock";
import { useApp, useDaemon } from "../../app/context";
import type { Session } from "../../ipc/generated/Session";
import type { SessionId } from "../../ipc/generated/SessionId";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { branchOf } from "../../state/selectors";
import { focusListOf } from "../../ui/listPointer";
import { AgentLogo, DiffStat, NewsDot, PullRequest, StatusMark } from "../../ui/status";
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

function Asking({ session }: { session: Session }) {
  const what = doing(session);
  return (
    <>
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={session.harness} />
      <span className="mu-row-text">
        <span className="mu-cut queue-asking" title={session.title}>
          {session.title}
        </span>
        <span className="mu-meta mu-cut queue-ask" title={what}>
          {what}
        </span>
      </span>
      <Age since={session.status_since} />
    </>
  );
}

function Finished({ session }: { session: Session }) {
  const workspace = useDaemon((state) => state.workspaces[session.workspace]);
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
      <span className="mu-row-text">
        <span className="mu-cut" title={session.title}>
          {session.title}
        </span>
        <span className="mu-meta mu-cut" title={what}>
          {what}
        </span>
      </span>
      <Age since={session.status_since} />
    </>
  );
}

const SECTION_ROWS = { asking: Asking, news: Finished, working: Working } as const;

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
  const Content = SECTION_ROWS[section];
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
      className={`mu-row mu-row--two ${resting ? "is-resting" : ""} ${flash}`}
      onMouseDown={focusListOf}
      onClick={() => onPick(id)}
    >
      <Content session={session} />
    </button>
  );
}
