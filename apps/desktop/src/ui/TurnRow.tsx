import { useNow } from "../app/clock";
import { useDaemon } from "../app/context";
import type { Entry } from "../ipc/generated/Entry";
import { agentOf } from "../lib/agents";
import { ago } from "../lib/format";
import { ListItem, ListItemEnd, ListItemMeta, ListItemText, ListItemTitle } from "./List";
import { AgentLogo, DiffStat } from "./status";

const AGENTS = ["claude", "codex", "gemini", "opencode", "copilot"];

/** The harness of an agent the timeline names, where its session has gone. */
function harnessNamed(agent: string): string {
  return AGENTS.find((id) => agentOf(id).name === agent) ?? agent;
}

/** A turn's line's id in the window, for its list's `aria-activedescendant`. */
export function turnLineId(number: number | string): string {
  return `turn-${number}`;
}

/** One entry of a workspace's timeline: its number, who, what it changed, and when it ended. A click opens its diff. */
export function TurnRow({
  entry,
  label,
  selected,
  onOpen,
}: {
  entry: Entry;
  label: string;
  selected: boolean;
  onOpen(): void;
}) {
  const now = useNow();
  const session = entry.kind.kind === "turn" ? entry.kind.session : "";
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const files = entry.summary?.files ?? 0;
  return (
    <ListItem
      id={turnLineId(entry.number)}
      selected={selected}
      label={`${entry.number} ${label}`}
      density="compact"
      className="mu-turnrow"
      onSelect={onOpen}
    >
      <span className="mu-turnrow-num">{entry.number}</span>
      {entry.kind.kind === "turn" ? (
        <AgentLogo harness={harness || harnessNamed(entry.kind.agent)} size="sm" />
      ) : (
        <span className="mu-logo mu-logo--sm mu-logo--mark" aria-hidden="true">
          {entry.kind.kind === "undo" ? "↺" : entry.kind.kind === "moved" ? "⎇" : "✎"}
        </span>
      )}
      <ListItemText>
        <ListItemTitle>{label}</ListItemTitle>
        <ListItemMeta>
          {entry.summary ? (
            <>
              {files} {files === 1 ? "file" : "files"}{" "}
              <DiffStat added={entry.summary.added} removed={entry.summary.removed} />
            </>
          ) : (
            "changed no files"
          )}
        </ListItemMeta>
      </ListItemText>
      <ListItemEnd title={new Date(entry.ended).toLocaleString()}>
        {ago(now - entry.ended)}
      </ListItemEnd>
    </ListItem>
  );
}
