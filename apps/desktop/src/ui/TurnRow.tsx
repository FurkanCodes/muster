import { useNow } from "../app/clock";
import { useDaemon } from "../app/context";
import type { Entry } from "../ipc/generated/Entry";
import { ago } from "../lib/format";
import { AgentLogo, DiffStat } from "./status";

export function TurnRow({
  entry,
  label,
  selected,
  onSelect,
  onOpen,
}: {
  entry: Entry;
  label: string;
  selected: boolean;
  onSelect(): void;
  onOpen(): void;
}) {
  const now = useNow();
  const session = entry.kind.kind === "turn" ? entry.kind.session : "";
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      tabIndex={selected ? 0 : -1}
      className="mu-turnrow"
      onFocus={onSelect}
      onClick={onOpen}
    >
      <span className="mu-turnrow-num">{entry.number}</span>
      {entry.kind.kind === "turn" ? (
        <AgentLogo harness={harness || entry.kind.agent.toLowerCase()} size="sm" />
      ) : (
        <span className="mu-logo mu-logo--sm" aria-hidden="true">
          {entry.kind.kind === "between" ? "✎" : "↺"}
        </span>
      )}
      <span className="mu-turnrow-text">
        <span>{label}</span>
        <span className="mu-meta">
          {entry.summary ? (
            <>
              <span>
                {entry.summary.files} {entry.summary.files === 1 ? "file" : "files"}{" "}
              </span>
              <DiffStat added={entry.summary.added} removed={entry.summary.removed} />
            </>
          ) : (
            "changed no files"
          )}
        </span>
      </span>
      <span className="mu-age" title={new Date(entry.ended).toLocaleString()}>
        {ago(now - entry.ended)}
      </span>
    </button>
  );
}
