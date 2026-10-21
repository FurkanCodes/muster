import type { Entry } from "../ipc/generated/Entry";
import type { FileChange } from "../ipc/generated/FileChange";
import type { Timeline } from "../ipc/generated/Timeline";

/** Same numbering as muster_core::timeline::turn_of_session, oldest turn is 1. */
export function turnLabel(timeline: Timeline, entry: Entry): string {
  const kind = entry.kind;
  switch (kind.kind) {
    case "turn": {
      const turn = timeline.entries.filter(
        (other) =>
          other.number <= entry.number &&
          other.kind.kind === "turn" &&
          other.kind.session === kind.session,
      ).length;
      return `${kind.agent}, turn ${turn}`;
    }
    case "between":
      return "Edits between turns";
    case "undo":
      return `Put back as before #${kind.of}`;
    case "moved":
      return `Moved onto ${kind.onto}`;
  }
}

/** Git quotes paths as C strings, including UTF-8 octal bytes. */
function gitPath(raw: string): string {
  if (!raw.startsWith('"')) return raw;
  const bytes: number[] = [];
  const encoder = new TextEncoder();
  const escapes: Record<string, string> = {
    t: "\t",
    n: "\n",
    r: "\r",
    b: "\b",
    f: "\f",
    v: "\v",
    a: String.fromCharCode(7),
  };
  for (let at = 1; at < raw.length - 1; ) {
    if (raw[at] === "\\") {
      const octal = /^[0-7]{1,3}/.exec(raw.slice(at + 1))?.[0];
      if (octal) {
        bytes.push(Number.parseInt(octal, 8));
        at += octal.length + 1;
        continue;
      }
      const next = raw[at + 1] ?? "";
      bytes.push(...encoder.encode(escapes[next] ?? next));
      at += 2;
    } else {
      const character = String.fromCodePoint(raw.codePointAt(at) ?? 0);
      bytes.push(...encoder.encode(character));
      at += character.length;
    }
  }
  return new TextDecoder().decode(Uint8Array.from(bytes));
}

/** Git's +++/--- headers give paths, including deleted and quoted paths. */
export function filesInDiff(text: string): string[] {
  const files: string[] = [];
  let before: string | undefined;
  let headers = false;
  const path = (raw: string) => {
    const value = gitPath(raw.split("\t")[0] ?? "");
    return value === "/dev/null" ? undefined : value.replace(/^[ab]\//, "");
  };
  for (const line of text.split("\n")) {
    if (line.startsWith("diff --git ")) {
      headers = true;
      before = undefined;
    }
    if (line.startsWith("@@")) headers = false;
    if (!headers) continue;
    if (line.startsWith("--- ")) before = path(line.slice(4));
    if (line.startsWith("+++ ")) {
      const file = path(line.slice(4)) ?? before;
      if (file && !files.includes(file)) files.push(file);
    }
    if (line.startsWith("Binary files ")) {
      const match = /^Binary files (.+) and (.+) differ$/.exec(line);
      const file = match ? (path(match[2] ?? "") ?? path(match[1] ?? "")) : undefined;
      if (file && !files.includes(file)) files.push(file);
    }
    if (line.startsWith("rename to ") || line.startsWith("copy to ")) {
      const file = gitPath(line.slice(line.startsWith("rename") ? 10 : 8));
      if (file && !files.includes(file)) files.push(file);
    }
  }
  return files;
}

/** A file a turn changed, with its lines (none for a binary file). */
export type TurnFile = Pick<FileChange, "path" | "lines">;

/** Each file of a turn's diff and its lines, counted in its hunks only. */
export function turnFiles(text: string): TurnFile[] {
  return text.split(/\n(?=diff --git )/).flatMap((section) => {
    const path = filesInDiff(section)[0];
    if (!path) return [];
    let hunk = false;
    let added = 0;
    let removed = 0;
    for (const line of section.split("\n")) {
      if (line.startsWith("@@")) {
        hunk = true;
        continue;
      }
      if (!hunk) continue;
      if (line.startsWith("+")) added += 1;
      else if (line.startsWith("-")) removed += 1;
    }
    return [{ path, lines: section.includes("Binary files ") ? null : { added, removed } }];
  });
}
