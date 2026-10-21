// The shape of the flows' questions (design system: Dialog): the answer that
// does nothing first, then the one that acts, named for what it does. Focus
// starts on the acting answer, or on the other where the acting one is
// danger. While the daemon answers, the acting answer says what is
// happening and a second press does nothing; a refusal stays in the dialog.

import type { KeyboardEvent, ReactNode } from "react";
import { useActions } from "../app/context";
import { Button } from "../ui/controls";
import { Dialog } from "../ui/Dialog";
import type { useTask } from "./useTask";
import "./flows.css";

export function Ask({
  title,
  act,
  acting,
  keep = "Keep it",
  tone = "primary",
  task,
  onAct,
  onKeyDown,
  children,
}: {
  title: string;
  /** The acting answer: "Close session". */
  act: string;
  /** What it says while the daemon answers: "Closing…". */
  acting: string;
  keep?: string;
  tone?: "primary" | "danger";
  task: ReturnType<typeof useTask>;
  onAct: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLDialogElement>) => void;
  children?: ReactNode;
}) {
  const actions = useActions();
  const close = () => {
    if (!task.busy) actions.openFlow(null);
  };
  const danger = tone === "danger";
  return (
    <Dialog
      title={title}
      onClose={close}
      onAccept={onAct}
      onKeyDown={onKeyDown}
      actions={
        <>
          <Button
            variant="secondary"
            size="lg"
            title={`${keep} (⎋)`}
            disabled={task.busy}
            data-dialog-initial={danger ? true : undefined}
            onClick={close}
          >
            {keep}
          </Button>
          <Button
            variant={tone}
            size="lg"
            title={`${act} (⏎)`}
            busy={task.busy}
            data-dialog-initial={danger ? undefined : true}
            onClick={onAct}
          >
            {task.busy ? acting : act}
          </Button>
        </>
      }
    >
      {children}
      {task.error ? (
        <p className="mu-dialog-text ui-flow-error" role="alert">
          {task.error}
        </p>
      ) : null}
    </Dialog>
  );
}
