export interface DiffLine {
  kind: "context" | "add" | "del" | "hunk" | "header";
  text: string;
  old: number | null;
  next: number | null;
  offset: number;
  height: number;
}

/** Git's unified diff, with positions for a variable-height window. */
export function parseDiff(text: string): DiffLine[] {
  let old = 0;
  let next = 0;
  let inHunk = false;
  let offset = 0;
  const source = text.split("\n");
  if (source.at(-1) === "") source.pop();
  return source.map((raw) => {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(raw);
    let kind: DiffLine["kind"] = "header";
    let before: number | null = null;
    let after: number | null = null;
    if (hunk) {
      old = Number(hunk[1]);
      next = Number(hunk[2]);
      inHunk = true;
      kind = "hunk";
    } else if (raw.startsWith("diff ")) inHunk = false;
    else if (inHunk && raw.startsWith("+")) {
      kind = "add";
      after = next++;
    } else if (inHunk && raw.startsWith("-")) {
      kind = "del";
      before = old++;
    } else if (inHunk && raw.startsWith(" ")) {
      kind = "context";
      before = old++;
      after = next++;
    }
    const height = kind === "hunk" ? 30 : 18;
    const line: DiffLine = {
      kind,
      text: (kind === "add" || kind === "del" || kind === "context" ? raw.slice(1) : raw).replace(
        /\t/g,
        "    ",
      ),
      old: before,
      next: after,
      offset,
      height,
    };
    offset += height;
    return line;
  });
}

export function lineAt(lines: DiffLine[], offset: number): number {
  let low = 0;
  let high = lines.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    const line = lines[mid];
    if (line && line.offset + line.height < offset) low = mid + 1;
    else high = mid;
  }
  return low;
}
