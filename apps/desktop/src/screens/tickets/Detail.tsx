import { useEffect } from "react";
import { useStore } from "zustand";
import { useShallow } from "zustand/react/shallow";
import { useNow } from "../../app/clock";
import { useDaemon, useUi } from "../../app/context";
import { Page } from "../../app/Page";
import type { Issue } from "../../ipc/generated/Issue";
import { agentOf } from "../../lib/agents";
import { ago, spend } from "../../lib/format";
import { doing, statusKind } from "../../lib/status";
import { Button, IconButton } from "../../ui/controls";
import { Heading, HeadingActions, HeadingLead, HeadingTitle } from "../../ui/Heading";
import { Label, Labels } from "../../ui/Label";
import { ListItemEnd, ListItemMeta, ListItemText, ListItemTitle, RowButton } from "../../ui/List";
import { Loading } from "../../ui/Loading";
import {
  AgentLogo,
  BranchName,
  DiffStat,
  ErrorText,
  PullRequest,
  StatusMark,
} from "../../ui/status";
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
    <Page label={`Ticket ${shownKey(issue.key)}`} className="tickets-detail">
      <Heading className="tickets-detail-heading">
        <HeadingTitle>{ticketText(shown.title)}</HeadingTitle>
        <HeadingLead>
          <span className="tickets-detail-facts">
            <span className="mu-mono">{shownKey(issue.key)}</span>
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
            <Labels>
              {shown.labels.map((label) => (
                <Label key={label}>{ticketText(label)}</Label>
              ))}
            </Labels>
          </span>
        </HeadingLead>
        <HeadingActions>
          {agents.length ? (
            <Button icon="plus" onClick={onAssign} title={`Assign another agent (${keys.another})`}>
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
        </HeadingActions>
      </Heading>
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
          <Loading label="Fetching its text" />
        ) : text.state === "failed" ? (
          <ErrorText>
            Its text could not be fetched: {text.error}. {keys.refresh} tries now.
          </ErrorText>
        ) : (
          <TicketText body={text.value.body} />
        )}
      </div>
    </Page>
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
    <RowButton className="tickets-agent-row" title="Open its agent (⏎)" onClick={() => onOpen(id)}>
      <StatusMark kind={statusKind(session)} />
      <AgentLogo harness={session.harness} />
      <ListItemText>
        <ListItemTitle>
          {label ?? agentOf(session.harness).name}{" "}
          <span className="mu-quiet">{ticketText(doing(session))}</span>
        </ListItemTitle>
        <ListItemMeta>
          <BranchName>{ticketText(workspace?.kind.branch ?? "")}</BranchName>
          {shared && workspace ? <ProjectName id={workspace.project} /> : null}
          {workspace?.changes ? (
            <DiffStat added={workspace.changes.added} removed={workspace.changes.removed} />
          ) : null}
          {workspace?.pull_request ? <PullRequest pr={workspace.pull_request} /> : null}
          {session.spend ? (
            <span>{spend(session.spend.tokens, session.spend.cost_micros)}</span>
          ) : null}
        </ListItemMeta>
      </ListItemText>
      <ListItemEnd>{ago(now - session.status_since)}</ListItemEnd>
    </RowButton>
  );
}
