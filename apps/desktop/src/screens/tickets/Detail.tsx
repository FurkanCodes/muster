import { useEffect } from "react";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { useNow } from "../../app/clock";
import { useDaemon, useUi } from "../../app/context";
import type { Issue } from "../../ipc/generated/Issue";
import { agentOf } from "../../lib/agents";
import { ago, spend } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { Button, IconButton } from "../../ui/controls";
import { AgentLogo, DiffStat, PullRequest, StatusMark } from "../../ui/status";
import { agentsOn, shownKey, worktreesFor } from "./agents";
import { type TicketsModel, ticketId } from "./data";
import { ProjectName } from "./TicketRow";
import { TicketText, ticketText } from "./TicketText";
import { oneTicketOf } from "./words";

export function Detail({
  issue,
  projects,
  tracker,
  model,
  onAssign,
  onOpen,
  onClose,
  onBrowser,
  keys,
}: {
  issue: Issue;
  projects: readonly string[];
  tracker: string;
  model: TicketsModel;
  onAssign: () => void;
  onOpen: (id: string) => void;
  onClose: () => void;
  onBrowser: () => void;
  keys: { another: string; browser: string; close: string; refresh: string };
}) {
  const project = projects[0] ?? "";
  const text = useStore(model.store, (s) => s.texts[ticketId(project, issue.key)]);
  const visible = useUi((s) => s.visible);
  const worktrees = useDaemon(
    useShallow((s) => worktreesFor(Object.values(s.workspaces), projects, issue.key)),
  );
  const agents = useDaemon(useShallow((s) => agentsOn(Object.values(s.sessions), worktrees)));
  const total = agents.reduce((sum, s) => sum + (s.spend?.tokens ?? 0), 0);
  const costs = agents.every((s) => s.spend?.cost_micros !== null && s.spend !== null);
  const cost = costs ? agents.reduce((sum, s) => sum + (s.spend?.cost_micros ?? 0), 0) : null;
  useEffect(() => {
    if (visible && !text) void model.view(project, issue);
  }, [visible, model, project, issue, text]);
  const shown = text?.state === "loaded" ? text.value.issue : issue;
  return (
    <section className="mu-pane tickets-detail" aria-label={`Ticket ${shownKey(issue.key)}`}>
      <div className="mu-pane-head tickets-detail-head">
        <div className="mu-pane-title-row tickets-title-row">
          <span className="mu-mono mu-quiet">{shownKey(issue.key)}</span>
          <h2 className="mu-pane-title mu-pane-title--wrap">{ticketText(shown.title)}</h2>
          <div className="mu-pane-actions">
            {agents.length ? (
              <Button
                icon="plus"
                onClick={onAssign}
                title={`Assign another agent (${keys.another})`}
              >
                Another agent
              </Button>
            ) : null}
            {/^https?:\/\//i.test(issue.url) ? (
              <IconButton
                icon="external"
                label={`Open in the browser (${keys.browser})`}
                onClick={onBrowser}
              />
            ) : null}
            <IconButton
              icon="more"
              label={`More: close the ticket (${keys.close})`}
              onClick={onClose}
            />
          </div>
        </div>
        <div className="mu-pane-meta">
          <span>
            {oneTicketOf(tracker)} in{" "}
            {projects.map((id, i) => (
              <span key={id}>
                {i ? ", " : ""}
                <ProjectName id={id} />
              </span>
            ))}
          </span>
          {shown.author ? <span>opened by {ticketText(shown.author)}</span> : null}
          <span className="mu-labels">
            {shown.labels.map((label) => (
              <span className="mu-label" key={label}>
                {ticketText(label)}
              </span>
            ))}
          </span>
        </div>
      </div>
      <div className="tickets-detail-body">
        {agents.length ? (
          <section aria-labelledby="ticket-agents" className="tickets-agents">
            <div className="tickets-agents-head">
              <h3 id="ticket-agents" className="title-section">
                Agents
              </h3>
              {total ? <span className="mu-meta">{spend(total, cost)}</span> : null}
            </div>
            {agents.map((s) => (
              <AgentRow key={s.id} id={s.id} shared={projects.length > 1} onOpen={onOpen} />
            ))}
          </section>
        ) : (
          <section className="tickets-unassigned">
            <span>
              <strong>No agent on it yet</strong>
              <span className="mu-meta">
                An agent gets a worktree of its own and the ticket as its first prompt.
              </span>
            </span>
            <Button variant="primary" onClick={onAssign} title="Assign an agent (⏎)">
              Assign an agent
            </Button>
          </section>
        )}
        <div className="tickets-rule" />
        {!text || text.state === "loading" ? (
          <div role="status" aria-label="Fetching its text…" className="tickets-loading">
            <span className="mu-meta">Fetching its text…</span>
            <span />
            <span />
            <span />
          </div>
        ) : text.state === "failed" ? (
          <p className="tickets-error" role="alert">
            Its text could not be fetched: {text.error}. {keys.refresh} tries now.
          </p>
        ) : (
          <TicketText body={text.value.body} />
        )}
      </div>
    </section>
  );
}

function AgentRow({
  id,
  shared,
  onOpen,
}: {
  id: string;
  shared: boolean;
  onOpen: (id: string) => void;
}) {
  const session = useDaemon((s) => s.sessions[id]);
  const workspace = useDaemon((s) => (session ? s.workspaces[session.workspace] : undefined));
  const now = useNow();
  const label = useDaemon(
    (s) => s.preferences?.agents.find((a) => a.name === session?.harness)?.label,
  );
  if (!session) return null;
  return (
    <button
      type="button"
      className="mu-row mu-row--two tickets-agent-row"
      onClick={() => onOpen(id)}
      title="Open its agent (⏎)"
    >
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={session.harness} />
      <span className="mu-row-text">
        <span className="mu-cut">
          {label ?? agentOf(session.harness).name}{" "}
          <span className="mu-quiet">{ticketText(doing(session))}</span>
        </span>
        <span className="mu-row-line mu-meta">
          <span className="mu-branch mu-cut">⎇ {ticketText(workspace?.kind.branch ?? "")}</span>
          {shared && workspace ? <ProjectName id={workspace.project} /> : null}
          {workspace?.changes ? (
            <DiffStat added={workspace.changes.added} removed={workspace.changes.removed} />
          ) : null}
          {workspace?.pull_request ? <PullRequest pr={workspace.pull_request} /> : null}
          {session.spend ? (
            <span>{spend(session.spend.tokens, session.spend.cost_micros)}</span>
          ) : null}
        </span>
      </span>
      <span className="mu-age">{ago(now - session.status_since)}</span>
    </button>
  );
}
