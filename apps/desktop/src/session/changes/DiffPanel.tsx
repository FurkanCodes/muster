import type { Command } from "../../ipc/generated/Command";
import { DiffView } from "../../ui/DiffView";
import { useReply } from "./useReply";

export function DiffPanel({
  command,
  revision,
  title,
  part,
}: {
  command: Command;
  revision: string;
  title: string;
  part: string;
}) {
  const answer = useReply(command, revision);
  if (answer.state === "loading")
    return (
      <p className="mu-meta" role="status">
        Asking git…
      </p>
    );
  if (answer.state === "error")
    return (
      <p className="mu-meta" role="alert">
        {answer.error} r asks again.
      </p>
    );
  if (typeof answer.value === "string" || !("FileDiff" in answer.value))
    return <p role="alert">The daemon did not send a diff.</p>;
  const diff = answer.value.FileDiff;
  const binary = /^(?:Binary files .+ differ|GIT binary patch)/m.test(diff.text);
  return binary ? (
    <div className="mu-diffview changes-diff">
      <div className="mu-diffview-head">{title}</div>
      <p className="mu-meta">binary — Git shows no text diff for this file.</p>
    </div>
  ) : (
    <DiffView key={`${JSON.stringify(command)}:${revision}`} diff={diff}>
      <span className="mu-mono mu-cut changes-diff-title" title={title}>
        {title}
      </span>
      <span className="mu-quiet">{part}</span>
    </DiffView>
  );
}
