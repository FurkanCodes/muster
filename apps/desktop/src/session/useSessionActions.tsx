// What a list does to its selected session on a key, as the terminal UI
// does it: R renames it in place, x closes it (after asking, while it
// runs), b its pull request in the browser (byHand.ts). Closing one that
// runs stops it and takes it off the list once its program has ended,
// wherever the person has gone by then.

import { useEffect, useRef, useState } from "react";
import { type App, useApp, useDaemon } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { Button } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import { TextField } from "../ui/TextField";
import { browsePullRequest } from "./byHand";

/** Says a refusal as the daemon gave it, in the status bar until the next key. */
function refused(app: App, error: unknown): void {
  app.actions.tell(error instanceof Error ? error.message : String(error), true);
}

/** Once `session`'s program has ended, takes it off the list (`RemoveSession`). */
function removeWhenEnded(app: App, session: SessionId): void {
  const ended = () => {
    const stored = app.daemon.store.getState().sessions[session];
    return !stored || stored.ended_at !== null || stored.status.state === "exited";
  };
  const remove = () => {
    if (!app.daemon.store.getState().sessions[session]) return;
    app.bridge.command({ RemoveSession: { session } }).then(
      () => app.actions.tell("Closed the session."),
      (error) => refused(app, error),
    );
  };
  if (ended()) {
    remove();
    return;
  }
  const stop = app.daemon.store.subscribe(() => {
    if (!ended()) return;
    stop();
    remove();
  });
}

export function useSessionActions(session: SessionId | undefined) {
  const app = useApp();
  const [renaming, setRenaming] = useState<SessionId | null>(null);
  const [closing, setClosing] = useState<SessionId | null>(null);
  const title = useDaemon((state) => (session ? (state.sessions[session]?.title ?? "") : ""));

  const stored = () => (session ? app.daemon.store.getState().sessions[session] : undefined);
  return {
    session,
    title,
    renaming,
    closing,
    rename() {
      if (!stored()) return app.actions.tell("Select a session to rename it.", true);
      setRenaming(session ?? null);
    },
    stopRenaming: () => setRenaming(null),
    async saveName(name: string): Promise<string | null> {
      if (!renaming) return null;
      try {
        await app.bridge.command({ RenameSession: { session: renaming, title: name.trim() } });
        setRenaming(null);
        app.actions.tell("Renamed.");
        return null;
      } catch (error) {
        return error instanceof Error ? error.message : String(error);
      }
    },
    close() {
      const found = stored();
      if (!found) return app.actions.tell("Select a session to close it.", true);
      // An ended session leaves the list at once; one that runs asks first.
      if (found.ended_at !== null || found.status.state === "exited") {
        app.bridge.command({ RemoveSession: { session: found.id } }).then(
          () => app.actions.tell("Removed the session."),
          (error) => refused(app, error),
        );
        return;
      }
      setClosing(found.id);
    },
    keepIt: () => setClosing(null),
    confirmClose() {
      if (!closing) return;
      const id = closing;
      setClosing(null);
      app.actions.tell("Closing the session…");
      app.bridge.command({ StopSession: { session: id } }).then(
        () => removeWhenEnded(app, id),
        (error) => refused(app, error),
      );
    },
    browser: () => browsePullRequest(app),
  };
}

export type SessionActions = ReturnType<typeof useSessionActions>;

/** "Close Sketch the Tauri shell?", asked before a session that runs is stopped. */
export function CloseSession({ actions }: { actions: SessionActions }) {
  if (!actions.closing) return null;
  return (
    <Dialog
      title={`Close ${actions.title}?`}
      onClose={actions.keepIt}
      onAccept={actions.confirmClose}
      actions={
        <>
          <Button
            variant="secondary"
            data-dialog-initial
            title="Keep it (⎋)"
            onClick={actions.keepIt}
          >
            Keep it
          </Button>
          <Button variant="danger" title="Close the session (⏎)" onClick={actions.confirmClose}>
            Close session
          </Button>
        </>
      }
    >
      <p className="mu-dialog-text">
        Its agent stops and it leaves the list. Its worktree and branch stay as they are.
      </p>
    </Dialog>
  );
}

/** The session's name, edited in place of its row: ⏎ keeps it, ⎋ gives up. */
export function RenameSession({ actions }: { actions: SessionActions }) {
  const [name, setName] = useState(actions.title);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const form = useRef<HTMLFormElement>(null);
  useEffect(() => {
    const input = form.current?.querySelector("input");
    input?.focus();
    input?.select();
  }, []);
  return (
    <form
      ref={form}
      className="queue-rename"
      onSubmit={(event) => {
        event.preventDefault();
        // Nothing to call it: the field stays for a name, as in the terminal UI.
        if (!name.trim() || busy) return;
        setBusy(true);
        void actions.saveName(name).then((refusal) => {
          setBusy(false);
          setError(refusal);
        });
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        event.stopPropagation();
        actions.stopRenaming();
      }}
    >
      <TextField
        label="Session name"
        name="session-name"
        value={name}
        error={error}
        hint="⏎ renames it, ⎋ leaves it as it was"
        onChange={(value) => {
          setName(value);
          setError(null);
        }}
        onBlur={actions.stopRenaming}
      />
    </form>
  );
}
