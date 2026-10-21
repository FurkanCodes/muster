import type { Clash } from "../../ipc/generated/Clash";
import type { Entry } from "../../ipc/generated/Entry";
import type { Timeline } from "../../ipc/generated/Timeline";

export function oneLine(text: string): string {
  return text.replace(/[\p{Cc}\p{Cf}\p{Zl}\p{Zp}]/gu, " ");
}

/** Same wording and kept-turn numbering as app::timeline::label in the TUI. */
export function entryLabel(timeline: Timeline, entry: Entry): string {
  const kind = entry.kind;
  switch (kind.kind) {
    case "turn":
      return `${oneLine(kind.agent)} · turn ${timeline.entries.filter((other) => other.number <= entry.number && other.kind.kind === "turn" && other.kind.session === kind.session).length}`;
    case "between":
      return "Edits between turns";
    case "undo":
      return `Put back as before #${kind.of}`;
    case "moved":
      return `Moved onto ${oneLine(kind.onto)}`;
  }
}

/** TUI view::clashes::what: three paths, then the remainder in words. */
export function clashWords(clash: Clash): string {
  const conflict = clash.conflicts > 0;
  const paths = clash.shown.filter((file) => file.conflict || !conflict).slice(0, 3);
  const more = Math.max(0, (conflict ? clash.conflicts : clash.files) - paths.length);
  return `${conflict ? "would conflict in" : "changes them too:"} ${paths.map((file) => oneLine(file.path)).join(", ")}${more ? ` and ${more} more` : ""}`;
}
