// Putting a workspace's files back as they were before an entry of its
// timeline (PRD-14), as the TUI does it (app::timeline): the daemon is
// asked what that would do (UndoPlan), the person is asked, and Undo is
// sent with or without the branch. A refusal, of the plan or of the undo,
// is said in the status bar, as the TUI says it.

import { useEffect, useRef, useState } from "react";
import { useNow } from "../../app/clock";
import { useActions, useBridge } from "../../app/context";
import type { Entry } from "../../ipc/generated/Entry";
import type { Timeline } from "../../ipc/generated/Timeline";
import type { UndoPlan } from "../../ipc/generated/UndoPlan";
import { ago } from "../../lib/format";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { entryLabel, oneLine, turnOf } from "./labels";

/** The key that puts a turn back (`muster_core::keys`, PutBack). */
export const PUT_BACK_KEY = "z";

function failure(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** What the files go back to, for the question: `Codex's turn 2`. */
export function beforeWhat(timeline: Timeline, entry: Entry | undefined, number: number): string {
  if (!entry) return `#${number}`;
  switch (entry.kind.kind) {
    case "turn":
      return `${oneLine(entry.kind.agent)}'s turn ${turnOf(timeline, entry)}`;
    case "between":
      return "the edits between turns";
    case "undo":
      return `putting back #${entry.kind.of}`;
    case "moved":
      return `moving onto ${oneLine(entry.kind.onto)}`;
  }
}

/** The branch's line in the dialog, as the TUI's `undo_lines` says it. */
export function branchWords(branch: NonNullable<UndoPlan["branch"]>): string {
  const name = `⎇ ${oneLine(branch.branch)}`;
  if (branch.refused) return `${name} stays where it is: ${oneLine(branch.refused)}.`;
  const commits = `${branch.commits} ${branch.commits === 1 ? "commit" : "commits"}`;
  return branch.forward
    ? `Put back with the branch also moves ${name} forward to ${oneLine(branch.to)}, over ${commits}.`
    : `Put back with the branch also moves ${name} back to ${oneLine(branch.to)}, before its ${commits} since.`;
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
  const [plan, setPlan] = useState<UndoPlan | null>(null);
  const closing = useRef({ onClose, actions });
  closing.current = { onClose, actions };
  useEffect(() => {
    let current = true;
    bridge.command({ UndoPlan: { workspace, number } }).then(
      (reply) => {
        if (!current) return;
        if (typeof reply !== "string" && "UndoPlan" in reply) setPlan(reply.UndoPlan);
        else {
          closing.current.actions.tell("The daemon did not say what putting back would do.", true);
          closing.current.onClose();
        }
      },
      (error: unknown) => {
        if (!current) return;
        closing.current.actions.tell(failure(error), true);
        closing.current.onClose();
      },
    );
    return () => {
      current = false;
    };
  }, [bridge, workspace, number]);
  return plan ? (
    <PlanDialog
      workspace={workspace}
      plan={plan}
      timeline={timeline}
      onClose={onClose}
      onDone={onDone}
    />
  ) : null;
}

function PlanDialog({
  workspace,
  plan,
  timeline,
  onClose,
  onDone,
}: {
  workspace: string;
  plan: UndoPlan;
  timeline: Timeline;
  onClose(): void;
  onDone(entry: number): void;
}) {
  const actions = useActions();
  const bridge = useBridge();
  const now = useNow();
  const [busy, setBusy] = useState<"files" | "branch" | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    actions.keysTo("dialog");
    return () => {
      mounted.current = false;
      actions.keysTo("list");
    };
  }, [actions]);
  const entry = timeline.entries.find((found) => found.number === plan.number);
  const canMove = plan.branch !== null && plan.branch.refused === null;
  const later = timeline.entries
    .filter((other) => other.number > plan.number && other.summary !== null)
    .sort((a, b) => a.number - b.number);
  const undo = async (moveBranch: boolean) => {
    if (busy || (moveBranch && !canMove)) return;
    setBusy(moveBranch ? "branch" : "files");
    try {
      const reply = await bridge.command({
        Undo: { workspace, number: plan.number, move_branch: moveBranch },
      });
      if (!mounted.current) return;
      if (typeof reply === "string" || !("Undone" in reply))
        throw new Error("The daemon did not say the files were put back.");
      const branch = reply.Undone.moved_branch ? ", and the branch with them" : "";
      actions.tell(
        `The files are as they were before #${plan.number}${branch}. ${PUT_BACK_KEY} on #${reply.Undone.entry} brings them back.`,
      );
      onDone(reply.Undone.entry);
    } catch (error) {
      if (!mounted.current) return;
      actions.tell(failure(error), true);
      onClose();
    }
  };
  const answer = (moveBranch: boolean) => {
    const which = moveBranch ? "branch" : "files";
    const primary = moveBranch === canMove;
    return (
      <Button
        variant={primary ? "primary" : "secondary"}
        size="lg"
        busy={busy === which}
        disabled={busy !== null}
        data-dialog-initial={primary || undefined}
        onClick={() => void undo(moveBranch)}
        title={moveBranch ? "Files and branch (b)" : "Files only (y)"}
      >
        {busy === which
          ? "Putting back…"
          : moveBranch
            ? "Put back with the branch"
            : "Put back the files"}
      </Button>
    );
  };
  return (
    <Dialog
      width="wide"
      title={`Put the files back as before ${beforeWhat(timeline, entry, plan.number)}?`}
      onClose={() => {
        if (!busy) onClose();
      }}
      onAccept={() => void undo(canMove)}
      onKeyDown={(event) => {
        const does = { y: false, b: true } as Record<string, boolean>;
        if (event.key === "n") {
          event.preventDefault();
          if (!busy) onClose();
        } else if (event.key in does) {
          event.preventDefault();
          void undo(does[event.key] ?? false);
        }
      }}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            disabled={busy !== null}
            onClick={onClose}
            title="Keep it (n)"
          >
            Keep it
          </Button>
          {answer(false)}
          {canMove ? answer(true) : null}
        </>
      }
    >
      <p className="mu-dialog-text">
        {entry
          ? `The worktree's files go back as they were ${ago(now - entry.started)} ago.`
          : `The worktree's files go back as they were before #${plan.number}.`}{" "}
        What is there now is kept first, so this can be put back too.
      </p>
      <dl className="mu-details">
        {plan.later > 0 ? (
          <>
            <dt>Goes with it</dt>
            <dd>
              {later.map((other) => (
                <span key={other.number} className="put-back-line">
                  {entryLabel(timeline, other)}, {ago(now - other.ended)} ago
                </span>
              ))}
              {plan.later > later.length ? (
                <span className="put-back-line">
                  {later.length === 0
                    ? `What ${plan.later} later ${plan.later === 1 ? "entry" : "entries"} changed`
                    : `and ${plan.later - later.length} more`}
                </span>
              ) : null}
            </dd>
          </>
        ) : null}
        {plan.branch ? (
          <>
            <dt>The branch</dt>
            <dd>{branchWords(plan.branch)}</dd>
          </>
        ) : null}
        {entry?.kind.kind === "turn" ? (
          <>
            <dt>{oneLine(entry.kind.agent)}</dt>
            <dd>Still remembers that turn: tell it what you put back.</dd>
          </>
        ) : null}
      </dl>
    </Dialog>
  );
}
