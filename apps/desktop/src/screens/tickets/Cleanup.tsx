import { useCallback, useEffect, useRef, useState } from "react";
import { useActions, useApp, useBridge } from "../../app/context";
import { type Bridge, BridgeFailure } from "../../ipc/bridge";
import type { Issue } from "../../ipc/generated/Issue";
import type { Workspace } from "../../ipc/generated/Workspace";
import { errorText } from "../../lib/text";
import { standsBeside } from "../../state/selectors";
import { Button } from "../../ui/controls";
import { Detail, DetailList } from "../../ui/DetailList";
import { Dialog, DialogText } from "../../ui/Dialog";
import { BranchName, ErrorText } from "../../ui/status";
import { shownKey } from "./agents";
import type { TicketsModel } from "./data";
import { ticketText } from "./TicketText";
import { repositoryOf, trackerName } from "./words";

/** Closes the ticket `key` in `project`'s tracker, or throws why not. */
async function closeTicket(bridge: Bridge, project: string, key: Issue["key"]): Promise<void> {
  const reply = await bridge.command({ CloseIssue: { project, key } });
  if (reply !== "Done") throw new Error("The daemon did not close the ticket.");
}

export function CloseTicket({
  issue,
  projects,
  tracker,
  worktrees,
  model,
  onClose,
}: {
  issue: Issue;
  projects: readonly string[];
  tracker: string;
  worktrees: readonly Workspace[];
  model: TicketsModel;
  onClose: () => void;
}) {
  const bridge = useBridge();
  const app = useApp();
  const actions = useActions();
  const [phase, setPhase] = useState<"close" | "offer" | "cleaning">("close");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const key = shownKey(issue.key);
  const repository = repositoryOf(issue.url);
  const working = Object.values(app.daemon.store.getState().sessions).filter(
    (s) => worktrees.some((w) => w.id === s.workspace) && s.ended_at === null && !standsBeside(s),
  );
  async function close() {
    if (busy) return;
    setBusy(true);
    setError(null);
    const project = projects[0] ?? "";
    try {
      await closeTicket(bridge, project, issue.key);
      model.closed(projects, issue.key);
      if (!worktrees.length) {
        actions.tell(`Closed ${key} in ${tracker}.`);
        onClose();
      } else {
        setPhase("offer");
        setBusy(false);
      }
    } catch (error) {
      setError(errorText(error));
      setBusy(false);
    }
  }
  if (phase === "cleaning") return <Cleanup worktrees={worktrees} ticket={key} onClose={onClose} />;
  return (
    <Dialog
      key={phase}
      title={
        phase === "close"
          ? `Close ${key} on ${trackerName(tracker)}?`
          : `Closed ${key}. Clean up after it?`
      }
      onClose={() => {
        if (!busy) onClose();
      }}
      onAccept={() => {
        if (phase === "close") void close();
        else setPhase("cleaning");
      }}
      actions={
        phase === "close" ? (
          <>
            <Button
              variant="secondary"
              size="lg"
              data-dialog-initial
              disabled={busy}
              onClick={onClose}
              title="Keep it open (⎋)"
            >
              Keep it open
            </Button>
            <Button
              variant="danger"
              size="lg"
              busy={busy}
              disabled={busy}
              onClick={() => void close()}
              title="Close ticket (⌘⏎)"
            >
              {busy ? "Closing…" : "Close ticket"}
            </Button>
          </>
        ) : (
          <>
            <Button variant="secondary" size="lg" onClick={onClose} title="Leave them (⎋)">
              Leave them
            </Button>
            <Button
              variant="primary"
              size="lg"
              data-dialog-initial
              onClick={() => setPhase("cleaning")}
              title="Clean up (⏎)"
            >
              Clean up
            </Button>
          </>
        )
      }
    >
      {phase === "close" ? (
        <DialogText>
          Everyone watching{" "}
          {repository ? (
            <>
              <span className="mu-mono">{ticketText(repository)}</span>{" "}
            </>
          ) : null}
          {key} is told. Muster changes nothing else in the tracker.
        </DialogText>
      ) : (
        <DetailList className="tickets-cleanup">
          {working.length ? (
            <Detail name="Stops">
              {working.length === 1 ? "1 agent" : `${working.length} agents`} still running:{" "}
              {working.map((s) => ticketText(s.title)).join(", ")}
            </Detail>
          ) : null}
          <Detail name="Removes">
            {worktrees.map((w, i) => (
              <span key={w.id}>
                {i ? (i === worktrees.length - 1 ? " and " : ", ") : null}
                <BranchName>{ticketText(w.kind.branch)}</BranchName>
              </span>
            ))}
            , once nothing runs in {worktrees.length === 1 ? "it" : "them"}
          </Detail>
          <Detail name="Keeps">
            {worktrees.length === 1
              ? "its branch"
              : worktrees.length === 2
                ? "both branches"
                : `all ${worktrees.length} branches`}
          </Detail>
        </DetailList>
      )}
      {error ? <ErrorText>{error}</ErrorText> : null}
    </Dialog>
  );
}

/** Subscribes to end updates before stopping; Done alone never means idle. */
function Cleanup({
  worktrees,
  ticket,
  onClose,
}: {
  worktrees: readonly Workspace[];
  ticket: string;
  onClose: () => void;
}) {
  const app = useApp();
  const bridge = useBridge();
  const actions = useActions();
  const [dirty, setDirty] = useState<Workspace | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const started = useRef(false);
  const pending = useRef(new Set(worktrees.map((w) => w.id)));
  const inFlight = useRef(new Set<string>());
  const refused = useRef(new Set<string>());
  const dirtyQueue = useRef<Workspace[]>([]);
  const alive = useRef(true);
  const kept = useRef(false);
  const finish = useCallback(() => {
    if (!pending.current.size && alive.current) {
      actions.tell(
        kept.current
          ? `Finished cleaning up after ${ticket}. Kept the worktrees you chose and all branches.`
          : `Cleaned up after ${ticket}. Branches are kept.`,
      );
      onClose();
    }
  }, [actions, ticket, onClose]);
  useEffect(() => {
    alive.current = true;
    async function settle() {
      const state = app.daemon.store.getState();
      for (const id of pending.current) {
        if (!alive.current) return;
        if (inFlight.current.has(id) || refused.current.has(id)) continue;
        if (Object.values(state.sessions).some((s) => s.workspace === id && s.ended_at === null))
          continue;
        inFlight.current.add(id);
        try {
          if (state.workspaces[id])
            await bridge.command({ RemoveWorktree: { workspace: id, force: false } });
          pending.current.delete(id);
        } catch (error) {
          refused.current.add(id);
          if (
            error instanceof BridgeFailure &&
            error.code === "conflict" &&
            error.message.includes("uncommitted changes")
          ) {
            const workspace = worktrees.find((w) => w.id === id);
            if (workspace) {
              dirtyQueue.current.push(workspace);
              if (alive.current) setDirty((current) => current ?? dirtyQueue.current[0] ?? null);
            }
          } else if (alive.current) setError(errorText(error));
        }
        // Whatever came of it: every failure is caught above.
        inFlight.current.delete(id);
      }
      finish();
    }
    const unsubscribe = app.daemon.store.subscribe(() => {
      void settle();
    });
    if (!started.current) {
      started.current = true;
      const running = Object.values(app.daemon.store.getState().sessions).filter(
        (s) => pending.current.has(s.workspace) && s.ended_at === null,
      );
      for (const session of running)
        void bridge.command({ StopSession: { session: session.id } }).catch((error) => {
          if (alive.current) setError(errorText(error));
        });
    }
    void settle();
    return () => {
      alive.current = false;
      unsubscribe();
    };
  }, [app, bridge, worktrees, finish]);
  function next(keep = true) {
    if (keep) kept.current = true;
    if (dirty) pending.current.delete(dirty.id);
    dirtyQueue.current.shift();
    setDirty(dirtyQueue.current[0] ?? null);
    finish();
  }
  async function force() {
    if (!dirty || busy) return;
    setBusy(true);
    setError(null);
    try {
      await bridge.command({ RemoveWorktree: { workspace: dirty.id, force: true } });
      next(false);
    } catch (error) {
      setError(errorText(error));
    }
    setBusy(false);
  }
  return (
    <Dialog
      key={dirty?.id ?? "cleaning"}
      title={dirty ? `Remove ⎇ ${ticketText(dirty.kind.branch)}?` : `Cleaning up after ${ticket}…`}
      onClose={() => {
        if (!busy) {
          if (dirty) next();
          else onClose();
        }
      }}
      onAccept={() => {
        if (dirty) void force();
      }}
      actions={
        dirty ? (
          <>
            <Button
              variant="secondary"
              size="lg"
              data-dialog-initial
              disabled={busy}
              onClick={() => next()}
              title="Keep it (⎋)"
            >
              Keep it
            </Button>
            <Button
              variant="danger"
              size="lg"
              disabled={busy}
              busy={busy}
              onClick={() => void force()}
              title="Remove with its changes (⌘⏎)"
            >
              Remove with its changes
            </Button>
          </>
        ) : (
          <Button variant="secondary" size="lg" data-dialog-initial onClick={onClose}>
            Leave the rest
          </Button>
        )
      }
    >
      <DialogText>
        {dirty
          ? `It has changes not committed. Removing deletes its folder and those changes. The branch stays.`
          : "Stopping its agents. Its worktrees are removed once nothing runs in them. Branches are kept."}
      </DialogText>
      {error ? <ErrorText>{error}</ErrorText> : null}
    </Dialog>
  );
}
