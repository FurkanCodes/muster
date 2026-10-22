// Making a worktree (`w`, a project page's New worktree), as the TUI's
// dialog does (crates/muster-tui/src/app/events.rs): which project, and the
// branch, checked out if it exists and made from the newest of the branch
// it merges into if not. Work starts in one place, the project's page
// (screens/sessions/ProjectPage.tsx); this only makes the place.

import { useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon } from "../app/context";
import type { ProjectId } from "../ipc/generated/ProjectId";
import { projectsInOrder } from "../state/selectors";
import { whenKnown } from "../state/whenKnown";
import { Button } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import { Select } from "../ui/Select";
import { TextField } from "../ui/TextField";
import { makeWorktree } from "./start";
import type { Flow } from "./types";
import { useTask } from "./useTask";
import "./flows.css";

export default function NewWorktree({ flow }: { flow: Extract<Flow, { kind: "new-worktree" }> }) {
  const app = useApp();
  const actions = useActions();
  const projects = useDaemon(useShallow(projectsInOrder)).map((project) => ({
    value: project.id,
    label: project.name,
  }));
  const [project, setProject] = useState<ProjectId>(flow.project);
  const [branch, setBranch] = useState("");
  const task = useTask();
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const submit = () =>
    task.run(async () => {
      const made = await makeWorktree(app, project, branch);
      actions.openFlow(null);
      actions.show("all");
      whenKnown(app, "workspaces", made, () => actions.select("all", made));
      actions.tell("Made the worktree.");
    });

  return (
    <Dialog
      title="New worktree"
      onClose={close}
      onAccept={() => void submit()}
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
            title="Make the worktree (⏎)"
            busy={task.busy}
            data-dialog-initial
            onClick={() => void submit()}
          >
            {task.busy ? "Making…" : "Make the worktree"}
          </Button>
        </>
      }
    >
      <div className="mu-field">
        <span className="mu-field-label">Project</span>
        <Select
          label="Project"
          options={projects}
          value={project}
          disabled={task.busy}
          onChange={setProject}
        />
      </div>
      <TextField
        label={
          <>
            Branch <span className="mu-quiet ui-flow-quiet">if you like</span>
          </>
        }
        hint="Checked out if it exists; else made from the newest of the branch it merges into."
        mono
        placeholder="muster/ and six letters, if left empty"
        autoComplete="off"
        spellCheck={false}
        value={branch}
        onChange={setBranch}
      />
      {task.error ? (
        <p className="mu-dialog-text ui-flow-error" role="alert">
          {task.error}
        </p>
      ) : null}
    </Dialog>
  );
}
