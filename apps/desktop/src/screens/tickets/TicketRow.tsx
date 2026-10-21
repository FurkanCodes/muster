import { useShallow } from "zustand/react/shallow";
import { useDaemon } from "../../app/context";
import type { Issue } from "../../ipc/generated/Issue";
import { statusKind } from "../../lib/status";
import { projectColour } from "../../state/selectors";
import { AgentLogo, PullRequest, StatusMark } from "../../ui/status";
import { agentsOn, shownKey, ticketPullRequest, worktreesFor } from "./agents";
import { ticketText } from "./TicketText";

export function TicketRow({
  issue,
  projects,
  selected,
  onSelect,
}: {
  issue: Issue;
  projects: readonly string[];
  selected: boolean;
  onSelect: () => void;
}) {
  const worktrees = useDaemon(
    useShallow((s) => worktreesFor(Object.values(s.workspaces), projects, issue.key)),
  );
  const agents = useDaemon(useShallow((s) => agentsOn(Object.values(s.sessions), worktrees)));
  const pr = ticketPullRequest(worktrees);
  return (
    <button
      type="button"
      className="mu-ticket"
      aria-label={`${shownKey(issue.key)} ${ticketText(issue.title)}`}
      aria-current={selected || undefined}
      onClick={onSelect}
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
    </button>
  );
}

export function ProjectName({ id }: { id: string }) {
  const name = useDaemon((s) => s.projects[id]?.name ?? "?");
  const colour = useDaemon((s) => projectColour(s, id));
  return <span className={`mu-p${colour}`}>{name}</span>;
}

export function ProjectSwatch({ id }: { id: string }) {
  const colour = useDaemon((s) => projectColour(s, id));
  return <span className={`mu-swatch mu-p${colour}`} aria-hidden="true" />;
}
