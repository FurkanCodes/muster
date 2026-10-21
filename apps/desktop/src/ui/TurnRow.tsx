import { useNow } from "../app/clock";
import { useDaemon } from "../app/context";
import type { Entry } from "../ipc/generated/Entry";
import { agentOf } from "../lib/agents";
import { ago } from "../lib/format";
import { AgentLogo, DiffStat } from "./status";

const AGENTS = ["claude", "codex", "gemini", "opencode", "copilot"];

/** The harness of an agent the timeline names, where its session has gone. */
function harnessNamed(agent: string): string {
  return AGENTS.find((id) => agentOf(id).name === agent) ?? agent;
}

/** One entry of a workspace's timeline: its number, who, what it changed, and when it ended. */
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
  const files = entry.summary?.files ?? 0;
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
        <AgentLogo harness={harness || harnessNamed(entry.kind.agent)} size="sm" />
      ) : (
        <span className="mu-logo mu-logo--sm mu-logo--mark" aria-hidden="true">
          {entry.kind.kind === "undo" ? "↺" : entry.kind.kind === "moved" ? "⎇" : "✎"}
        </span>
      )}
      <span className="mu-turnrow-text">
        <span>{label}</span>
        <span className="mu-meta">
          {entry.summary ? (
            <>
              {files} {files === 1 ? "file" : "files"}{" "}
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
