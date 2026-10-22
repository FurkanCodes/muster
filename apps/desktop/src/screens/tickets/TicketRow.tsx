import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon } from "../../app/context";
import type { Issue } from "../../ipc/generated/Issue";
import { statusKind } from "../../lib/status";
import { projectColour } from "../../state/selectors";
import { ListItem } from "../../ui/List";
import { AgentLogo, PullRequest, StatusMark, Swatch } from "../../ui/status";
import { changedInSight, stamp } from "../queue/history";
import { agentsOn, shownKey, ticketPullRequest, worktreesFor } from "./agents";
import { ticketText } from "./TicketText";

export function TicketRow({
  lineId,
  issue,
  projects,
  selected,
  onSelect,
}: {
  /** Its id in the window, for its list's `aria-activedescendant`. */
  lineId: string;
  issue: Issue;
  projects: readonly string[];
  selected: boolean;
  onSelect: () => void;
}) {
  const app = useApp();
  const worktrees = useDaemon(
    useShallow((s) => worktreesFor(Object.values(s.workspaces), projects, issue.key)),
  );
  const agents = useDaemon(useShallow((s) => agentsOn(Object.values(s.sessions), worktrees)));
  const pr = ticketPullRequest(worktrees);
  // An agent on it that came to need the person as they watched flashes
  // the row once, as the queue's rows flash.
  const asking = agents.findLast(
    (s) => ["waiting", "error"].includes(statusKind(s)) && changedInSight(app.daemon, s),
  );
  return (
    <ListItem
      // Drawn afresh for each new need, so each flashes once.
      key={asking ? stamp(asking) : "still"}
      id={lineId}
      selected={selected}
      label={`${shownKey(issue.key)} ${ticketText(issue.title)}`}
      flash={asking ? "waiting" : undefined}
      className="mu-ticket"
      onSelect={onSelect}
    >
      <span className="mu-ticket-key">{shownKey(issue.key)}</span>
      <span className="mu-ticket-text">
        <span className="mu-ticket-title">{ticketText(issue.title)}</span>
        {agents.length ? (
          <span className="mu-ticket-agents">
            {agents.slice(0, 4).map((s) => (
              <span key={s.id} className="mu-ticket-agent">
                <StatusMark kind={statusKind(s)} />
                <AgentLogo harness={s.harness} size="sm" />
                {projects.length > 1 ? (
                  <ProjectName id={worktrees.find((w) => w.id === s.workspace)?.project ?? ""} />
                ) : null}
              </span>
            ))}
            {agents.length > 4 ? <span className="mu-meta">+{agents.length - 4}</span> : null}
          </span>
        ) : null}
      </span>
      {pr ? (
        <span className="mu-ticket-end">
          <PullRequest pr={pr} />
        </span>
      ) : null}
    </ListItem>
  );
}

export function ProjectName({ id }: { id: string }) {
  const name = useDaemon((s) => s.projects[id]?.name ?? "?");
  const colour = useDaemon((s) => projectColour(s, id));
  return <span className={`mu-p${colour}`}>{name}</span>;
}

export function ProjectSwatch({ id }: { id: string }) {
  const colour = useDaemon((s) => projectColour(s, id));
  return <Swatch colour={colour} />;
}
