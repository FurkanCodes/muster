// The questions asked before Muster acts on a session, a worktree or a
// project, in the TUI's words where it asks them (crates/muster-tui/src/app/
// events.rs) and the boards' (design/screens/Dialogs.html): stopping (s),
// removing a worktree (with its changes, only when said) and no longer
// tracking a project. Renaming and closing a session are the lists' own
// (src/session/useSessionActions.tsx).

import { useState } from "react";
import { useApp, useDaemon } from "../app/context";
import { BridgeFailure } from "../ipc/bridge";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { DialogText } from "../ui/Dialog";
import { Ask } from "./Ask";
import { count, useChanges } from "./changes";
import { labelOf } from "./keymap";
import { useTask } from "./useTask";

export function Stop({ session }: { session: SessionId }) {
  const app = useApp();
  const title = useDaemon((state) => state.sessions[session]?.title ?? "the session");
  const restart = useDaemon((state) => labelOf(state, "restart_session"));
  const task = useTask();
  const stop = () =>
    void task.run(async () => {
      await app.bridge.command({ StopSession: { session } });
      app.actions.openFlow(null);
      app.actions.tell("Stopping the session.");
    });
  return (
    <Ask title={`Stop ${title}?`} act="Stop session" acting="Stopping…" task={task} onAct={stop}>
      <DialogText>
        Its program is told to stop. It stays on the list with its worktree, branch and transcript
        {restart === "none" ? "." : `, and ${restart} starts it again.`}
      </DialogText>
    </Ask>
  );
}

export function RemoveWorktree({ workspace }: { workspace: WorkspaceId }) {
  const app = useApp();
  const branch = useDaemon((state) => state.workspaces[workspace]?.kind.branch ?? "");
  const changes = useChanges(workspace);
  const [refused, setRefused] = useState(false);
  const task = useTask();
  const dirty = (changes?.uncommitted.length ?? 0) > 0;
  const force = dirty || refused;
  const remove = () =>
    void task.run(async () => {
      try {
        await app.bridge.command({ RemoveWorktree: { workspace, force } });
      } catch (error) {
        // Changes git found that were not counted: ask again, saying so.
        if (
          error instanceof BridgeFailure &&
          error.code === "conflict" &&
          error.message.includes("uncommitted")
        ) {
          setRefused(true);
          return;
        }
        throw error;
      }
      app.actions.openFlow(null);
      app.actions.tell(`Removed the worktree; ⎇ ${branch} stays.`);
    });
  const files = changes?.uncommitted.length ?? 0;
  return (
    <Ask
      title={`Remove ⎇ ${branch}?`}
      act={force ? "Remove with its changes" : "Remove worktree"}
      acting="Removing…"
      tone="danger"
      task={task}
      onAct={remove}
    >
      <DialogText>
        {force
          ? `It has ${files > 0 ? count(files, "file") : "files"} with changes not committed. Removing deletes its folder and those changes. The branch stays.`
          : "Removing deletes its folder. The branch stays."}
      </DialogText>
    </Ask>
  );
}

export function RemoveProject({ project }: { project: ProjectId }) {
  const app = useApp();
  const name = useDaemon((state) => state.projects[project]?.name ?? "this project");
  const task = useTask();
  const remove = () =>
    void task.run(async () => {
      await app.bridge.command({ RemoveProject: { project } });
      app.actions.openFlow(null);
      app.actions.tell("Muster no longer tracks the project; its folder is untouched.");
    });
  return (
    <Ask
      title={`Stop tracking ${name}?`}
      act="Stop tracking"
      acting="Stopping…"
      tone="danger"
      task={task}
      onAct={remove}
    >
      <DialogText>
        Muster forgets it and its ended sessions; nothing in its folder is touched. It can be added
        again.
      </DialogText>
    </Ask>
  );
}
