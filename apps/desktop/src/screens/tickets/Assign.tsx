import { useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useBridge, useDaemon } from "../../app/context";
import { ask } from "../../ipc/ask";
import type { Issue } from "../../ipc/generated/Issue";
import { useAsked } from "../../ipc/useAsked";
import { agentOf, OWN } from "../../lib/agents";
import { errorText } from "../../lib/text";
import { whenKnown } from "../../state/whenKnown";
import { AgentChoice } from "../../ui/AgentChoice";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { Select } from "../../ui/Select";
import { BranchName, ErrorText } from "../../ui/status";
import { Field } from "../../ui/TextField";
import { shownKey, ticketBranch, worktreesFor } from "./agents";
import { givenWords, TEXT_ALONE, type TicketsModel, ticketId } from "./data";
import { ticketText } from "./TicketText";

export function Assign({
  issue,
  projects,
  source,
  another,
  model,
  onClose,
}: {
  issue: Issue;
  projects: readonly string[];
  source: string | null;
  another: boolean;
  model: TicketsModel;
  onClose: () => void;
}) {
  const bridge = useBridge();
  const app = useApp();
  const actions = useActions();
  const added = useDaemon((s) => s.preferences?.agents);
  const workspaces = useDaemon(
    useShallow((s) => Object.values(s.workspaces).filter((w) => projects.includes(w.project))),
  );
  const names = useDaemon(useShallow((s) => projects.map((id) => s.projects[id]?.name ?? "?")));
  const last = model.store.getState().lastAgent;
  const offered = [
    ...OWN.map((id) => ({
      id,
      name: agentOf(id).name,
    })),
    ...(added ?? [])
      .filter((a) => a.takes_prompt && a.name !== "shell")
      .map((a) => ({ id: a.name, name: a.label })),
  ];
  const agents = offered
    .filter((a, index) => offered.findIndex((o) => o.id === a.id) === index)
    .sort((a, b) => Number(b.id === last) - Number(a.id === last));
  const [agent, setAgent] = useState(agents[0]?.id ?? "claude");
  // What it is given, where there are comments to choose among.
  const brief = (() => {
    const id = ticketId(projects[0] ?? "", issue.key);
    const { texts, briefs } = model.store.getState();
    const text = texts[id];
    return text?.state === "loaded" && text.value.comments?.length
      ? (briefs[id] ?? TEXT_ALONE)
      : null;
  })();
  const [project, setProject] = useState(() => {
    const remembered = source ? model.store.getState().lastProjects[source] : undefined;
    return remembered && projects.includes(remembered) ? remembered : (projects[0] ?? "");
  });
  const [on, setOn] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const read = useAsked(
    () => ask(bridge, { ProjectSettings: { project } }, "ProjectSettings"),
    project,
  );
  const settings = read.state === "answered" ? read.value : null;
  // Why it cannot start, or why the project's settings could not be read.
  const said = error ?? (read.state === "failed" ? read.error : null);
  const existing = worktreesFor(workspaces, [project], issue.key);
  const stacking = another || existing.length === 0;
  // The daemon gives another agent `…-2`, `…-3`: the first branch nobody has.
  const ownBranch = ticketBranch(issue.key, issue.title, settings?.muster_prefix?.on !== false);
  const ownOne = another && existing.length > 0;
  const taken = (branch: string) =>
    workspaces.some((w) => w.project === project && w.kind.branch === branch);
  let n = 2;
  let made = ownOne ? `${ownBranch}-${n}` : ownBranch;
  while (ownOne && taken(made)) made = `${ownBranch}-${++n}`;
  const checkout = workspaces.find((w) => w.project === project && w.kind.kind === "checkout");
  const choices = [
    { value: "", label: `⎇ ${checkout?.kind.branch || "the base"}`, note: "the base" },
    ...workspaces
      .filter(
        (w) =>
          w.project === project &&
          w.kind.kind === "worktree" &&
          w.kind.branch &&
          !w.kind.branch.startsWith("detached at "),
      )
      .map((w) => ({ value: w.id, label: `⎇ ${ticketText(w.kind.branch)}` })),
  ];
  const name = agents.find((a) => a.id === agent)?.name ?? agent;
  const chooseProject = (id: string) => {
    setProject(id);
    setOn("");
  };
  const step = <T,>(list: readonly T[], current: T, by: number) =>
    list[(list.indexOf(current) + by + list.length) % list.length] ?? current;
  async function start() {
    if (busy) return;
    setBusy(true);
    setError(null);
    // Worked out before the daemon is asked, as the compiler wants it.
    const startOn = stacking && on ? on : null;
    try {
      const reply = await ask(
        bridge,
        {
          StartFromIssue: {
            project,
            key: issue.key,
            harness: agent,
            size: { rows: 24, cols: 80 },
            another: ownOne,
            on: startOn,
            brief,
          },
        },
        "IssueStarted",
        "The daemon did not return its agent.",
      );
      model.store.setState((s) => ({
        lastAgent: agent,
        lastProjects: source ? { ...s.lastProjects, [source]: project } : s.lastProjects,
      }));
      const session = reply.session;
      actions.show("all");
      // Selected once the daemon has told it, which it does after its answer.
      whenKnown(app, "sessions", session, () => {
        actions.select("all", session);
        actions.showTerminal(session);
        actions.keysTo("list");
      });
      actions.tell(`Started ${name} on ${shownKey(issue.key)}.`);
      onClose();
    } catch (error) {
      setError(errorText(error));
      setBusy(false);
    }
  }
  return (
    <Dialog
      title={`Assign an agent to ${shownKey(issue.key)}`}
      width="wide"
      meta={ticketText(issue.title)}
      onClose={() => {
        if (!busy) onClose();
      }}
      onAccept={() => void start()}
      onKeyDown={(event) => {
        if (busy) return;
        if (event.key === "Tab") {
          event.preventDefault();
          setAgent(
            step(
              agents.map((a) => a.id),
              agent,
              event.shiftKey ? -1 : 1,
            ),
          );
        }
        if (event.target instanceof HTMLElement && event.target.closest(".ui-select")) return;
        if (["ArrowLeft", "ArrowRight"].includes(event.key)) {
          event.preventDefault();
          if (stacking)
            setOn(
              step(
                choices.map((c) => c.value),
                on,
                event.key === "ArrowLeft" ? -1 : 1,
              ),
            );
          else
            setAgent(
              step(
                agents.map((a) => a.id),
                agent,
                event.key === "ArrowLeft" ? -1 : 1,
              ),
            );
        }
        if (projects.length > 1 && ["ArrowUp", "ArrowDown"].includes(event.key)) {
          event.preventDefault();
          chooseProject(step(projects, project, event.key === "ArrowUp" ? -1 : 1));
        }
      }}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            disabled={busy}
            onClick={onClose}
            title="Cancel (⎋)"
          >
            Cancel
          </Button>
          <Button
            variant="primary"
            size="lg"
            data-dialog-initial
            disabled={busy}
            busy={busy}
            onClick={() => void start()}
            title={`Start ${name} (⏎)`}
          >
            {busy ? "Starting…" : `Start ${name}`}
          </Button>
        </>
      }
    >
      <Field label="Agent" hint="⇥ goes round the agents.">
        <AgentChoice
          agents={agents}
          value={agent}
          last={last}
          onChange={(id) => {
            if (!busy) setAgent(id);
          }}
        />
      </Field>
      {projects.length > 1 ? (
        <Field label="Project" hint="↑ ↓ chooses which project it is for.">
          <Select
            label="Project"
            value={project}
            options={projects.map((id, i) => ({ value: id, label: names[i] ?? "?" }))}
            disabled={busy}
            onChange={chooseProject}
          />
        </Field>
      ) : null}
      {stacking ? (
        <Field
          label="Made on top of"
          hint={
            <>
              A new worktree on <span className="mu-mono">{ticketText(made)}</span>. ← → stack it on
              another worktree's branch instead.
            </>
          }
        >
          <Select
            label="Made on top of"
            options={choices}
            value={on}
            disabled={busy}
            onChange={setOn}
          />
        </Field>
      ) : (
        <p className="mu-meta">
          In the ticket's worktree, on{" "}
          <BranchName>{ticketText(existing[0]?.kind.branch ?? "")}</BranchName>.
        </p>
      )}
      <p className="mu-meta">
        {name} gets {brief ? givenWords(brief) : "the ticket"} as its first prompt, marked as a
        description of the work, never as instructions.
      </p>
      {said ? <ErrorText>{said}</ErrorText> : null}
    </Dialog>
  );
}
