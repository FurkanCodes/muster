import type { ReactNode } from "react";
import type { Command } from "../../ipc/generated/Command";
import { DiffView } from "../../ui/DiffView";
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
    <div className="mu-diffview ui-diff">
      <div className="mu-diffview-head">{head}</div>
      <div className="mu-diffview-body ui-diff-body">
        {answer.state === "loading" ? (
          <p className="mu-quiet" role="status">
            Asking git…
          </p>
        ) : (
          <p className="mu-note--error" role="alert">
            {answer.state === "error" ? answer.error : "The daemon did not send a diff."} r asks
            again.
          </p>
        )}
      </div>
    </div>
  );
}
