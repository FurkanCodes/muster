import { useEffect, useRef, useState } from "react";
import { useActions, useBridge } from "../../app/context";
import type { Timeline } from "../../ipc/generated/Timeline";
import type { UndoPlan } from "../../ipc/generated/UndoPlan";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { entryLabel, oneLine } from "./labels";
import { useReply } from "./useReply";

function branchWords(branch: NonNullable<UndoPlan["branch"]>): string {
  const name = oneLine(branch.branch);
  if (branch.refused) return `${name} stays where it is: ${oneLine(branch.refused)}.`;
  const commits = `${branch.commits} ${branch.commits === 1 ? "commit" : "commits"}`;
  return branch.forward
    ? `Also moves ${name} forward to ${oneLine(branch.to)}, over ${commits}.`
    : `Also moves ${name} back to ${oneLine(branch.to)}, before its ${commits} since.`;
}

export function PutBack({
  workspace,
  number,
  timeline,
  onClose,
  onDone,
}: {
  workspace: string;
  number: number;
  timeline: Timeline;
  onClose(): void;
  onDone(entry: number): void;
}) {
  const actions = useActions();
  const bridge = useBridge();
  const [retry, setRetry] = useState(0);
  const answer = useReply({ UndoPlan: { workspace, number } }, String(retry));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const sending = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    actions.keysTo("dialog");
    return () => {
      mounted.current = false;
      actions.keysTo("list");
    };
  }, [actions]);
  const found =
    answer.state === "ready" && typeof answer.value !== "string" && "UndoPlan" in answer.value
      ? answer.value.UndoPlan
      : null;
  const entry = timeline.entries.find((entry) => entry.number === number);
  const label = entry ? entryLabel(timeline, entry) : `#${number}`;
  const undo = async (moveBranch: boolean) => {
    if (!found || sending.current || (moveBranch && (!found.branch || found.branch.refused)))
      return;
    sending.current = true;
    setBusy(true);
    setError(null);
    try {
      const reply = await bridge.command({
        Undo: { workspace, number: found.number, move_branch: moveBranch },
      });
      if (!mounted.current) return;
      if (typeof reply === "string" || !("Undone" in reply))
        throw new Error("The daemon did not say the files were put back.");
      actions.tell(
        `The files are as they were before #${found.number}${reply.Undone.moved_branch ? ", and the branch with them" : ""}. z on #${reply.Undone.entry} brings them back.`,
      );
      onDone(reply.Undone.entry);
    } catch (error) {
      if (mounted.current) setError(error instanceof Error ? error.message : String(error));
    } finally {
      sending.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  return (
    <Dialog
      focusKey={found?.number}
      title={`Put the files back as before ${label}?`}
      onCancel={() => {
        if (!busy) onClose();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (busy) {
          if (event.key === "Escape") event.preventDefault();
          return;
        }
        if (event.key === "Escape" || event.key === "n") {
          event.preventDefault();
          onClose();
        } else if (event.key === "Enter" || event.key === "y") {
          event.preventDefault();
          void undo(false);
        } else if (event.key === "b") {
          event.preventDefault();
          void undo(true);
        }
      }}
    >
      {answer.state === "loading" ? (
        <p role="status" className="mu-dialog-text">
          Asking what putting the files back would do…
        </p>
      ) : null}
      {answer.state === "error" ? (
        <>
          <p role="alert" className="mu-dialog-text">
            {answer.error}
          </p>
          <Button onClick={() => setRetry((count) => count + 1)} title="Ask for the plan again">
            Ask again
          </Button>
        </>
      ) : null}
      {found ? (
        <>
          <p className="mu-dialog-text">
            The files go back as they were before #{found.number} ({label}). What is there now is
            kept as an entry of its own, so this can be undone too.
          </p>
          <dl className="mu-details">
            {found.later ? (
              <>
                <dt>Goes with it</dt>
                <dd>
                  What {found.later} later {found.later === 1 ? "entry" : "entries"} changed goes
                  too.
                  {timeline.entries
                    .filter((other) => other.number > number && other.summary)
                    .map((other) => (
                      <div key={other.number}>{entryLabel(timeline, other)}</div>
                    ))}
                </dd>
              </>
            ) : null}
            {found.branch ? (
              <>
                <dt>The branch</dt>
                <dd>{branchWords(found.branch)}</dd>
              </>
            ) : null}
            {entry?.kind.kind === "turn" ? (
              <>
                <dt>{oneLine(entry.kind.agent)}</dt>
                <dd>
                  {oneLine(entry.kind.agent)} still remembers that turn: tell it what you put back.
                </dd>
              </>
            ) : null}
          </dl>
        </>
      ) : null}
      {error ? (
        <p className="mu-dialog-text" role="alert">
          {error}
        </p>
      ) : null}
      {busy ? (
        <p role="status" className="mu-meta">
          Putting the files back…
        </p>
      ) : null}
      <div className="mu-dialog-actions">
        <Button
          variant="secondary"
          size="lg"
          disabled={busy}
          onClick={onClose}
          title="Keep it (n, Esc)"
        >
          Keep it
        </Button>
        {found ? (
          <Button
            variant={found.branch && !found.branch.refused ? "secondary" : "primary"}
            size="lg"
            data-initial-focus
            disabled={busy}
            onClick={() => void undo(false)}
            title="Files only (y, Enter)"
          >
            Put back the files
          </Button>
        ) : null}
        {found?.branch && !found.branch.refused ? (
          <Button
            variant="primary"
            size="lg"
            disabled={busy}
            onClick={() => void undo(true)}
            title="Files and branch (b)"
          >
            Put back with the branch
          </Button>
        ) : null}
      </div>
    </Dialog>
  );
}
