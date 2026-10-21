// Starting a session (`n`) or making a worktree (`w`), as the TUI's dialogs
// do (crates/muster-tui/src/app/events.rs): where (a project, and its
// checkout, one of its worktrees or a new worktree on a new branch), what
// (an agent the daemon knows, the last chosen first, or a shell), and an
// optional first prompt, which names the chat. The daemon's own commands,
// CreateWorktree then StartSession; a worktree made before a launch the
// daemon refused is kept and started in on the next try, never made twice.

import { type KeyboardEvent, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon, useUi } from "../app/context";
import type { AddedAgent } from "../ipc/generated/AddedAgent";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { projectsInOrder } from "../state/selectors";
import { AgentChoice } from "../ui/AgentChoice";
import { Button, Segmented } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import { Select } from "../ui/Select";
import { TextArea, TextField } from "../ui/TextField";
import { START_SIZE } from "./actions";
import { offered, stepAgent } from "./agents";
import type { Flow } from "./types";
import { useTask } from "./useTask";
import "./flows.css";

type Where = "new" | "checkout" | "worktree";

const NO_AGENTS: AddedAgent[] = [];

/** The projects, in the daemon's order, as the select offers them. */
function useProjects() {
  const projects = useDaemon(useShallow(projectsInOrder));
  return projects.map((project) => ({ value: project.id, label: project.name }));
}

/** The project's checkout and worktrees: their ids, branches, and whether it is a plain folder. */
function usePlaces(project: ProjectId) {
  const checkout = useDaemon((state) =>
    Object.values(state.workspaces).find(
      (workspace) => workspace.project === project && workspace.kind.kind === "checkout",
    ),
  );
  const worktrees = useDaemon(
    useShallow((state) =>
      Object.values(state.workspaces).filter(
        (workspace) => workspace.project === project && workspace.kind.kind === "worktree",
      ),
    ),
  ).map((workspace) => ({ value: workspace.id, label: `⎇ ${workspace.kind.branch}` }));
  return { checkout, worktrees, plain: checkout?.kind.branch === "" };
}

export default function NewSession({
  flow,
}: {
  flow: Extract<Flow, { kind: "new-session" | "new-worktree" }>;
}) {
  const app = useApp();
  const actions = useActions();
  const making = flow.kind === "new-worktree";
  const projects = useProjects();
  const [project, setProject] = useState<ProjectId>(() => {
    const state = app.daemon.store.getState();
    const from = flow.kind === "new-session" ? flow.workspace : null;
    return flow.project ?? state.workspaces[from ?? ""]?.project ?? projects[0]?.value ?? "";
  });
  const { checkout, worktrees, plain } = usePlaces(project);
  const [where, setWhere] = useState<Where>(() => {
    const kind =
      flow.kind === "new-session"
        ? app.daemon.store.getState().workspaces[flow.workspace ?? ""]?.kind.kind
        : undefined;
    return kind === "checkout" ? "checkout" : kind === "worktree" ? "worktree" : "new";
  });
  const [worktree, setWorktree] = useState<WorkspaceId>(
    flow.kind === "new-session" ? (flow.workspace ?? "") : "",
  );
  const [branch, setBranch] = useState("");
  const [prompt, setPrompt] = useState("");
  const added = useDaemon((state) => state.preferences?.agents ?? NO_AGENTS);
  const last = useUi((state) => state.lastAgent);
  const agents = offered(added, last, "session");
  const [harness, setHarness] = useState(() => agents[0]?.id ?? "claude");
  const agent = agents.find((offer) => offer.id === harness) ?? agents[0];
  const task = useTask();

  // A plain folder has no worktrees: a session there starts in the folder itself.
  const place: Where = plain ? "checkout" : where === "worktree" && !worktree ? "new" : where;
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };

  const makeWorktree = async (): Promise<WorkspaceId> => {
    const reply = await app.bridge.command({
      CreateWorktree: { project, branch: branch.trim() || null, on: null },
    });
    if (typeof reply !== "object" || !("WorktreeCreated" in reply))
      throw new Error("The daemon answered something else; try again.");
    return reply.WorktreeCreated.id;
  };

  const submit = () =>
    task.run(async () => {
      if (!project) throw new Error("Add a project first.");
      if (making) {
        const made = await makeWorktree();
        actions.openFlow(null);
        actions.show("all");
        actions.select("all", made);
        actions.tell("Made the worktree.");
        return;
      }
      let workspace = place === "checkout" ? checkout?.id : place === "worktree" ? worktree : null;
      if (place === "new") {
        workspace = await makeWorktree();
        // Kept, so a refused launch is tried again in it, not in another new one.
        setWorktree(workspace);
        setWhere("worktree");
      }
      if (!workspace) throw new Error("Choose where it starts: its checkout or a worktree.");
      const reply = await app.bridge.command({
        StartSession: {
          workspace,
          harness,
          title: null,
          size: START_SIZE,
          prompt: agent?.takesPrompt ? prompt.trim() || null : null,
        },
      });
      if (typeof reply !== "object" || !("SessionStarted" in reply))
        throw new Error("The daemon answered something else; try again.");
      const session = reply.SessionStarted.id;
      actions.choseAgent(harness);
      actions.openFlow(null);
      actions.show("all");
      actions.select("all", session);
      actions.showTerminal(session);
      actions.keysTo("terminal");
    });

  // ←→ (and ↑↓) go round the agents while one has the focus.
  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    const by = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key] as
      | 1
      | -1
      | undefined;
    const target = event.target instanceof HTMLElement ? event.target : null;
    if (!by || !target?.closest("[role=radiogroup]")) return;
    event.preventDefault();
    const next = stepAgent(agents, harness, by);
    setHarness(next);
    const radios = target
      .closest("[role=radiogroup]")
      ?.querySelectorAll<HTMLElement>("[role=radio]");
    radios?.[agents.findIndex((offer) => offer.id === next)]?.focus();
  };

  const start = making
    ? "Make the worktree"
    : harness === "shell"
      ? "Start a shell"
      : `Start ${agent?.name ?? "the agent"}`;

  return (
    <Dialog
      title={making ? "New worktree" : "New session"}
      width="wide"
      onClose={close}
      onAccept={() => void submit()}
      onKeyDown={onKeyDown}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            title="Cancel (⎋)"
            disabled={task.busy}
            onClick={close}
          >
            Cancel
          </Button>
          <Button
            variant="primary"
            size="lg"
            title={`${start} (${making ? "⏎" : "⌘⏎"})`}
            busy={task.busy}
            disabled={projects.length === 0}
            data-dialog-initial={making || !agent?.takesPrompt ? true : undefined}
            onClick={() => void submit()}
          >
            {task.busy ? (making ? "Making…" : "Starting…") : start}
          </Button>
        </>
      }
    >
      {making ? null : (
        <TextArea
          label="First prompt"
          hint="It names the chat at once; an agent names it better a few seconds later."
          placeholder={
            agent?.takesPrompt
              ? "What should the agent do?"
              : `${agent?.name ?? "It"} takes no first prompt`
          }
          value={prompt}
          disabled={!agent?.takesPrompt}
          data-dialog-initial={agent?.takesPrompt ? true : undefined}
          onChange={setPrompt}
        />
      )}
      <div className="ui-flow-columns">
        <div className="mu-field">
          <span className="mu-field-label">Project</span>
          <Select
            label="Project"
            options={projects}
            value={project}
            disabled={task.busy}
            onChange={(next) => {
              setProject(next);
              setWhere("new");
              setWorktree("");
            }}
          />
        </div>
        {making ? null : (
          <div className="mu-field">
            <span className="mu-field-label">Where</span>
            {plain ? (
              <span className="mu-meta ui-flow-where">
                In the folder itself, as it is no repository
              </span>
            ) : (
              <Segmented<Where>
                label="Where"
                value={place}
                onChange={(next) => {
                  setWhere(next);
                  if (next === "worktree" && !worktree) setWorktree(worktrees[0]?.value ?? "");
                }}
                segments={[
                  { value: "new", label: "A new worktree" },
                  { value: "checkout", label: "Its checkout" },
                  ...(worktrees.length > 0
                    ? [{ value: "worktree" as const, label: "A worktree" }]
                    : []),
                ]}
              />
            )}
          </div>
        )}
      </div>
      {making || place === "new" ? (
        <TextField
          label={
            <>
              Branch <span className="mu-quiet ui-flow-quiet">if you like</span>
            </>
          }
          hint={
            making ? "Checked out if it exists; made from the checkout's newest if not." : undefined
          }
          mono
          placeholder="muster/ and six letters, if left empty"
          autoComplete="off"
          spellCheck={false}
          value={branch}
          onChange={setBranch}
        />
      ) : place === "worktree" ? (
        <div className="mu-field">
          <span className="mu-field-label">Worktree</span>
          <Select label="Worktree" options={worktrees} value={worktree} onChange={setWorktree} />
        </div>
      ) : null}
      {making ? null : (
        <div className="mu-field">
          <span className="mu-field-label">Agent</span>
          <AgentChoice agents={agents} value={harness} last={last} onChange={setHarness} />
        </div>
      )}
      {task.error ? (
        <p className="mu-dialog-text ui-flow-error" role="alert">
          {task.error}
        </p>
      ) : null}
    </Dialog>
  );
}
