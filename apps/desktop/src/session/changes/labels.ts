// What the changes view says, in the TUI's words (app::timeline::label,
// view::clashes, muster_core::changes) with the board's punctuation.

import type { ChangeSummary } from "../../ipc/generated/ChangeSummary";
import type { Changes } from "../../ipc/generated/Changes";
import type { Clash } from "../../ipc/generated/Clash";
import type { Entry } from "../../ipc/generated/Entry";
import type { Timeline } from "../../ipc/generated/Timeline";
import type { Workspace } from "../../ipc/generated/Workspace";

/** `muster_core::text::one_line`: line breaks and tabs as spaces, other control characters dropped. */
export function oneLine(text: string): string {
  return text.replace(/[\n\r\t]/g, " ").replace(/\p{Cc}/gu, "");
}

/** `timeline::turn_of_session`: a turn's place among its session's kept turns, from 1. */
export function turnOf(timeline: Timeline, entry: Entry): number | null {
  const kind = entry.kind;
  if (kind.kind !== "turn") return null;
  return timeline.entries.filter(
    (other) =>
      other.number <= entry.number &&
      other.kind.kind === "turn" &&
      other.kind.session === kind.session,
  ).length;
}

/** What an entry was, in a few words: `Codex, turn 3`. */
export function entryLabel(timeline: Timeline, entry: Entry): string {
  const kind = entry.kind;
  switch (kind.kind) {
    case "turn":
      return `${oneLine(kind.agent)} · turn ${turnOf(timeline, entry)}`;
    case "between":
      return "Edits between turns";
    case "undo":
      return `Put back as before #${kind.of}`;
    case "moved":
      return `Moved onto ${oneLine(kind.onto)}`;
  }
}

/** `Changes::summary`: what the listing comes to, as the workspace's line says it. */
export function summaryOf(changes: Changes): ChangeSummary | null {
  const files = [...changes.uncommitted, ...changes.branch];
  if (files.length === 0) return null;
  let added = 0;
  let removed = 0;
  for (const file of files) {
    added += file.lines?.added ?? 0;
    removed += file.lines?.removed ?? 0;
  }
  return { files: new Set(files.map((file) => file.path)).size, added, removed };
}

export function sameSummary(a: ChangeSummary | null, b: ChangeSummary | null): boolean {
  return (
    a === b ||
    (a !== null &&
      b !== null &&
      a.files === b.files &&
      a.added === b.added &&
      a.removed === b.removed)
  );
}

/** The folder every path is in, ending in `/`, or `""`: the board's "In crates/muster-desktop." */
export function commonFolder(paths: readonly string[]): string {
  const first = paths[0];
  if (first === undefined) return "";
  let folder = first.slice(0, first.lastIndexOf("/") + 1);
  for (const path of paths) {
    while (folder && !path.startsWith(folder))
      folder = folder.slice(0, folder.slice(0, -1).lastIndexOf("/") + 1);
  }
  return folder;
}

/** The other workspace as the TUI names it: `⎇ fix/resize`, `⌂ main`, `⧉ run`. */
export function placeName(workspace: Workspace | undefined): string {
  if (!workspace) return "another worktree";
  const symbol =
    workspace.kind.kind === "checkout" ? "⌂" : workspace.kind.kind === "container" ? "⧉" : "⎇";
  return `${symbol} ${oneLine(workspace.kind.branch)}`;
}

/** The most files a clash names; the rest are counted (view::clashes::NAMED). */
const NAMED = 3;

/** `Clash::would_conflict`. */
export function wouldConflict(clash: Clash): boolean {
  return clash.conflicts > 0;
}

/** The files a clash names, a few and the rest counted: `a.rs, b.rs and 3 more`. */
export function clashFiles(clash: Clash): string {
  const conflict = wouldConflict(clash);
  const named = clash.shown
    .filter((file) => file.conflict || !conflict)
    .slice(0, NAMED)
    .map((file) => oneLine(file.path));
  const more = Math.max(0, (conflict ? clash.conflicts : clash.files) - named.length);
  return `${named.join(", ")}${more ? ` and ${more} more` : ""}`;
}

/** What the banner says after the other workspace: the TUI's `what`, in the board's sentence. */
export function clashWords(clash: Clash): string {
  return wouldConflict(clash)
    ? `would conflict in ${clashFiles(clash)}`
    : `changes them too: ${clashFiles(clash)}`;
}
