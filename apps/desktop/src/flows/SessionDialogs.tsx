// The questions asked before Muster acts on a session, a worktree or a
// project, in the TUI's words where it asks them (crates/muster-tui/src/app/
// events.rs, rename.rs) and the boards' (design/screens/Dialogs.html):
// renaming (R), closing (x: stop, then remove once it has exited),
// stopping (s), removing a worktree (with its changes, only when said) and
// no longer tracking a project.

import { useEffect, useRef, useState } from "react";
import { type App, useApp, useDaemon } from "../app/context";
import { BridgeFailure } from "../ipc/bridge";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { Session } from "../ipc/generated/Session";
import type { SessionId } from "../ipc/generated/SessionId";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { agentOf } from "../lib/agents";
import { TextField } from "../ui/TextField";
import { Ask } from "./Ask";
import { count, useChanges } from "./changes";
import { labelOf } from "./keymap";
import { useTask } from "./useTask";

export function Rename({ session }: { session: SessionId }) {
  const app = useApp();
  const [name, setName] = useState(
    () => app.daemon.store.getState().sessions[session]?.title ?? "",
  );
  const [tried, setTried] = useState(false);
  const task = useTask();
  const empty = !name.trim();
  const rename = () => {
    setTried(true);
    // Nothing to call it: the field stays for a name, as in the TUI.
    if (empty) return;
    void task.run(async () => {
      await app.bridge.command({ RenameSession: { session, title: name.trim() } });
      app.actions.openFlow(null);
      app.actions.tell("Renamed.");
    });
  };
  return (
    <Ask
      title="Rename the session"
      act="Rename"
      acting="Renaming…"
      keep="Cancel"
      task={task}
      onAct={rename}
    >
      <TextField
        label="Name"
        value={name}
        maxLength={80}
        autoComplete="off"
        data-dialog-initial
        onFocus={(event) => event.currentTarget.select()}
        onChange={(event) => setName(event.target.value)}
        error={tried && empty ? "Name it with at least one letter." : null}
        hint="Up to 80 characters. No agent renames it after you do."
      />
    </Ask>
  );
}

/** Who is at work in a session, and how, for the question closing it. */
function atWork(session: Session): string {
  const who = session.harness === "shell" ? "A shell" : agentOf(session.harness).name;
  switch (session.status.state) {
    case "running":
      return `${who} is working in it.`;
    case "needs_you":
      return `${who} is waiting on you in it.`;
    case "fresh":
      return `${who} is starting in it.`;
    default:
      return `${who} is running in it.`;
  }
}

/** Resolves once the daemon tells the session has ended, or is gone. */
function ended(app: App, session: SessionId, signal: AbortSignal): Promise<void> {
  const done = () => {
    const found = app.daemon.store.getState().sessions[session];
    return !found || found.ended_at !== null;
  };
  if (done()) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const unsubscribe = app.daemon.store.subscribe(() => {
      if (!done()) return;
      unsubscribe();
      resolve();
    });
    signal.addEventListener("abort", () => {
      unsubscribe();
      reject(new Error("Closing was left."));
    });
  });
}

export function Close({ session }: { session: SessionId }) {
  const app = useApp();
  const found = useDaemon((state) => state.sessions[session]);
  const task = useTask();
  const leaving = useRef<AbortController | null>(null);
  useEffect(() => () => leaving.current?.abort(), []);
  const close = () =>
    void task.run(async () => {
      leaving.current = new AbortController();
      const now = app.daemon.store.getState().sessions[session];
      if (now && now.ended_at === null) {
        // The daemon says yes to a stop before the program has gone; it is
        // removed once its end is told, as the TUI's closing list does.
        await app.bridge.command({ StopSession: { session } });
        await ended(app, session, leaving.current.signal);
      }
      if (app.daemon.store.getState().sessions[session])
        await app.bridge.command({ RemoveSession: { session } });
      app.actions.openFlow(null);
      app.actions.tell("Closed the session.");
    });
  return (
    <Ask
      title={`Close ${found?.title ?? "the session"}?`}
      act="Close session"
      acting="Closing…"
      tone="danger"
      task={task}
      onAct={close}
    >
      <p className="mu-dialog-text">
        {found && found.ended_at === null ? `${atWork(found)} Closing stops it. ` : null}
        Its worktree, branch and transcript stay, so it can be started again.
      </p>
    </Ask>
  );
}

export function Stop({ session }: { session: SessionId }) {
  const app = useApp();
  const title = useDaemon((state) => state.sessions[session]?.title ?? "the session");
  const restart = useDaemon((state) => labelOf(state.preferences, "restart_session"));
  const task = useTask();
  const stop = () =>
    void task.run(async () => {
      await app.bridge.command({ StopSession: { session } });
      app.actions.openFlow(null);
      app.actions.tell("Stopping the session.");
    });
  return (
    <Ask title={`Stop ${title}?`} act="Stop session" acting="Stopping…" task={task} onAct={stop}>
      <p className="mu-dialog-text">
        Its program is told to stop. It stays on the list with its worktree, branch and transcript
        {restart === "none" ? "." : `, and ${restart} starts it again.`}
      </p>
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
      <p className="mu-dialog-text">
        {force
          ? `It has ${files > 0 ? count(files, "file") : "files"} with changes not committed. Removing deletes its folder and those changes. The branch stays.`
          : "Removing deletes its folder. The branch stays."}
      </p>
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
      <p className="mu-dialog-text">
        Muster forgets it and its ended sessions; nothing in its folder is touched. It can be added
        again.
      </p>
    </Ask>
  );
}
