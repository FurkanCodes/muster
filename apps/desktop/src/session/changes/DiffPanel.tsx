import type { ReactNode } from "react";
import type { Command } from "../../ipc/generated/Command";
import { DiffFrame, DiffHead, DiffView, DiffWaiting } from "../../ui/DiffView";
import { ErrorText } from "../../ui/status";
import { useReply } from "./useReply";

/** The diff of the file or entry opened, as the daemon answers `command` (FileDiff, EntryDiff). */
export function DiffPanel({
  command,
  revision,
  head,
  takeFocus,
  onLeave,
  onFocus,
}: {
  command: Command;
  revision: string;
  head: ReactNode;
  takeFocus: boolean;
  onLeave(): void;
  onFocus(): void;
}) {
  const answer = useReply(command, revision);
  if (answer.state === "ready" && typeof answer.value !== "string" && "FileDiff" in answer.value)
    return (
      <DiffView
        key={`${JSON.stringify(command)}:${revision}`}
        diff={answer.value.FileDiff}
        head={head}
        takeFocus={takeFocus}
        onLeave={onLeave}
        onFocus={onFocus}
      />
    );
  return (
    <DiffFrame>
      <DiffHead>{head}</DiffHead>
      <DiffWaiting>
        {answer.state === "loading" ? (
          <p className="mu-quiet" role="status">
            Asking git…
          </p>
        ) : (
          <ErrorText>
            {answer.state === "error" ? answer.error : "The daemon did not send a diff."} r asks
            again.
          </ErrorText>
        )}
      </DiffWaiting>
    </DiffFrame>
  );
}
