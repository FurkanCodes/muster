// The questions about spaces and terminals in no project (`ARC-33`), as the
// TUI asks them (crates/muster-tui/src/app/spaces.rs): a space's name, new
// or changed; whether to remove one (what is in it stays); and where a
// terminal in no project goes under a project: filed where its shell is,
// its folder made a project, or started again in one, saying what ends.
// Muster never types into the shell. The daemon refuses what it cannot do
// and says why.

import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon } from "../app/context";
import type { Reply } from "../ipc/generated/Reply";
import type { SessionId } from "../ipc/generated/SessionId";
import type { SpaceId } from "../ipc/generated/SpaceId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { errorText } from "../lib/text";
import { doLater } from "../spaces/spaces";
import { projectsInOrder } from "../state/selectors";
import { whenKnown } from "../state/whenKnown";
import { Button } from "../ui/controls";
import { Dialog, DialogAnswers, DialogText } from "../ui/Dialog";
import { ErrorText } from "../ui/status";
import { TextField } from "../ui/TextField";
import { Ask } from "./Ask";
import type { Flow, Later } from "./types";
import { useTask } from "./useTask";
import "./flows.css";

export function SpaceName({ space, after }: { space: SpaceId | null; after?: Later }) {
  const app = useApp();
  const actions = useActions();
  const was = useDaemon((state) => state.spaces.find((one) => one.id === space)?.name ?? "");
  const [name, setName] = useState(was);
  const task = useTask();
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const submit = () =>
    task.run(async () => {
      if (space === null) {
        const reply = await app.bridge.command({ CreateSpace: { name } });
        actions.openFlow(null);
        if (typeof reply === "object" && "SpaceCreated" in reply && after) {
          // What waited on a space goes in it; the window shows everything still.
          doLater(app, after, reply.SpaceCreated.id);
        } else {
          // Listed under its heading with the others; the list shows every space.
          actions.tell(`Made the space ${name.trim()}.`);
        }
      } else {
        await app.bridge.command({ RenameSpace: { space, name } });
        actions.openFlow(null);
        actions.tell("Renamed the space.");
      }
    });
  return (
    <Dialog
      title={space === null ? "New space" : `Rename ${was}`}
      meta={
        after
          ? "Everything lives in a space: name one, and what you asked for goes in it."
          : "A space holds projects and terminals; it doesn't change where they run."
      }
      onClose={close}
      onAccept={() => void submit()}
      actions={
        <DialogAnswers
          does={space === null ? "Make it" : "Rename"}
          doing={space === null ? "Making…" : "Renaming…"}
          busy={task.busy}
          disabled={!name.trim()}
          onCancel={close}
          onDo={() => void submit()}
        />
      }
    >
      <TextField
        label="Name"
        autoComplete="off"
        spellCheck={false}
        data-dialog-initial
        value={name}
        onChange={setName}
        error={task.error}
      />
    </Dialog>
  );
}

export function RemoveSpace({ space }: { space: SpaceId }) {
  const app = useApp();
  const name = useDaemon((state) => state.spaces.find((one) => one.id === space)?.name ?? "it");
  const task = useTask();
  const remove = () =>
    void task.run(async () => {
      await app.bridge.command({ DeleteSpace: { space } });
      app.actions.openFlow(null);
      if (app.ui.getState().space === space) app.actions.showSpace(null);
      app.actions.tell("Removed the space.");
    });
  return (
    <Ask
      title={`Remove ${name}?`}
      act="Remove the space"
      acting="Removing…"
      tone="danger"
      task={task}
      onAct={remove}
    >
      <DialogText>Only an empty space goes: move or remove what is in it first.</DialogText>
    </Ask>
  );
}

type Place = Extract<Reply, { Place: unknown }>["Place"];

/** Where the free terminal `session` goes under a project. */
export function FileTerminal({ flow }: { flow: Extract<Flow, { kind: "file-terminal" }> }) {
  const app = useApp();
  const actions = useActions();
  const session: SessionId = flow.session;
  const projects = useDaemon(useShallow(projectsInOrder));
  const workspaces = useDaemon((state) => state.workspaces);
  const [place, setPlace] = useState<Place | null>(null);
  const [restart, setRestart] = useState<WorkspaceId | null>(null);
  const task = useTask();
  useEffect(() => {
    let gone = false;
    app.bridge
      .command({ ReadPlace: { session } })
      .then((reply) => {
        if (!gone && typeof reply === "object" && "Place" in reply) setPlace(reply.Place);
      })
      .catch((error: unknown) => {
        if (!gone) actions.tell(errorText(error), true);
      });
    return () => {
      gone = true;
    };
  }, [app, actions, session]);
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const done = (said: string) => {
    actions.openFlow(null);
    actions.tell(said);
  };
  const fileUnder = (workspace: WorkspaceId) =>
    void task.run(async () => {
      await app.bridge.command({ FileSession: { session, workspace } });
      done("Filed under the project, still running.");
    });
  const addHere = () =>
    void task.run(async () => {
      const reply = await app.bridge.command({
        // The project goes in the terminal's space.
        AddProjectHere: { session, space: null },
      });
      done("Added the folder as a project, the terminal under it.");
      if (typeof reply === "object" && "ProjectAdded" in reply) {
        const id = reply.ProjectAdded.id;
        whenKnown(app, "projects", id, () => actions.select("all", id));
      }
    });
  const startAgain = (workspace: WorkspaceId) =>
    void task.run(async () => {
      await app.bridge.command({ RestartIn: { session, workspace } });
      done("Started it again in the project.");
    });
  // Each project's own checkout, else its first worktree.
  const firstOf = (project: string) =>
    Object.values(workspaces).find(
      (workspace) => workspace.project === project && workspace.kind.kind !== "container",
    )?.id ?? null;

  if (restart) {
    const ends = place?.front ? `${place.front} ends` : "Its shell ends";
    const name =
      projects.find((project) => project.id === workspaces[restart]?.project)?.name ?? "it";
    return (
      <Ask
        title={`Start it again in ${name}?`}
        act="End it and start again"
        acting="Starting…"
        tone="danger"
        task={task}
        onAct={() => startAgain(restart)}
      >
        <DialogText>
          {ends}, and a new terminal under the same name starts in {name}, shown where this one was.
          Background jobs aren't listed.
        </DialogText>
      </Ask>
    );
  }
  const holder = place?.workspace ? workspaces[place.workspace] : undefined;
  const holderName = holder
    ? projects.find((project) => project.id === holder.project)?.name
    : null;
  return (
    <Dialog
      title="Put it under a project"
      meta={place?.folder ? `Its shell is in ${place.folder}.` : "Reading where its shell is…"}
      onClose={close}
      onAccept={() => {
        // ⏎ does the first way, where there is one.
        if (holder && place?.workspace) fileUnder(holder.id);
        else if (place?.folder) addHere();
      }}
      actions={
        <Button variant="secondary" size="lg" onClick={close} disabled={task.busy}>
          Cancel
        </Button>
      }
    >
      <div className="ui-flow-choices">
        {holder && place?.workspace ? (
          <Button
            variant="primary"
            data-dialog-initial
            disabled={task.busy}
            onClick={() => fileUnder(holder.id)}
          >
            File under {holderName}, as it is
          </Button>
        ) : place?.folder ? (
          <Button variant="primary" data-dialog-initial disabled={task.busy} onClick={addHere}>
            Add {place.folder} as a project, the terminal under it
          </Button>
        ) : null}
        {projects
          .filter((project) => project.id !== holder?.project)
          .flatMap((project) => {
            const first = firstOf(project.id);
            return first
              ? [
                  <Button
                    key={project.id}
                    variant="secondary"
                    disabled={task.busy}
                    onClick={() => setRestart(first)}
                  >
                    Start again in {project.name}…
                  </Button>,
                ]
              : [];
          })}
      </div>
      {task.error ? <ErrorText>{task.error}</ErrorText> : null}
    </Dialog>
  );
}
