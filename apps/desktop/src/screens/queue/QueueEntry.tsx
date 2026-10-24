// One session of the queue, in the sidebar (design system: QueueRow): one
// asking something a two-line row with what it asks under its name, in full
// over its terminal; one that finished unseen a two-line row with its branch,
// what its work comes to and its pull request; one at work a two-line row
// with what it is doing.

import { useNow } from "../../app/clock";
import { useApp, useDaemon } from "../../app/context";
import type { Session } from "../../ipc/generated/Session";
import type { SessionId } from "../../ipc/generated/SessionId";
import { ago } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { branchOf, shownHarness } from "../../state/selectors";
import { ListItem, ListItemEnd, ListItemMeta, ListItemText, ListItemTitle } from "../../ui/List";
import { AgentLogo, BranchName, DiffStat, NewsDot, PullRequest, StatusMark } from "../../ui/status";
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
    <ListItemEnd>
      {ago(now - since)}
      {unseen ? <NewsDot /> : null}
    </ListItemEnd>
  );
}

function Asking({ session }: { session: Session }) {
  const what = doing(session);
  return (
    <>
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={shownHarness(session)} />
      <ListItemText>
        <ListItemTitle className="queue-asking" title={session.title}>
          {session.title}
        </ListItemTitle>
        <ListItemMeta title={what}>{what}</ListItemMeta>
      </ListItemText>
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
      <AgentLogo harness={shownHarness(session)} />
      <ListItemText>
        <ListItemTitle title={session.title}>{session.title}</ListItemTitle>
        <ListItemMeta>
          <BranchName>{workspace ? branchOf(workspace) : "no branch"}</BranchName>
          {changes && (changes.added || changes.removed) ? (
            <DiffStat added={changes.added} removed={changes.removed} />
          ) : (
            <span>no changes</span>
          )}
          {workspace?.pull_request ? <PullRequest pr={workspace.pull_request} /> : null}
        </ListItemMeta>
      </ListItemText>
      <Age since={session.status_since} unseen={session.unseen} />
    </>
  );
}

function Working({ session }: { session: Session }) {
  const what = doing(session);
  return (
    <>
      <StatusMark kind="working" />
      <AgentLogo harness={shownHarness(session)} />
      <ListItemText>
        <ListItemTitle title={session.title}>{session.title}</ListItemTitle>
        <ListItemMeta title={what}>{what}</ListItemMeta>
      </ListItemText>
      <Age since={session.status_since} />
    </>
  );
}

const SECTION_ROWS = { asking: Asking, news: Finished, working: Working } as const;

export function QueueEntry({
  id,
  selected,
  onPick,
}: {
  id: SessionId;
  selected: boolean;
  onPick: (id: SessionId) => void;
}) {
  const app = useApp();
  const session = useDaemon((state) => state.sessions[id]);
  if (!session) return null;
  const section = sectionOf(session);
  const kind = statusKind(session);
  const tone = kind in FLASHES ? FLASHES[kind as keyof typeof FLASHES] : undefined;
  const flash = tone && changedInSight(app.daemon, session) ? tone : undefined;
  const Content = SECTION_ROWS[section];
  return (
    <ListItem
      // A new status flashes once: the line is drawn afresh for it.
      key={flash ? stamp(session) : "still"}
      id={optionId(id)}
      selected={selected}
      label={session.title}
      flash={flash}
      onSelect={() => onPick(id)}
    >
      <Content session={session} />
    </ListItem>
  );
}
