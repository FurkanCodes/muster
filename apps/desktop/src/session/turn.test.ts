import { describe, expect, it } from "vitest";
import type { Entry } from "../ipc/generated/Entry";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { filesInDiff, turnFiles, turnLabel } from "./turn";

describe("ARC-24 a turn's label and files, as This turn shows them", () => {
  const entry = workingDay().timelines?.[ulid(11)]?.entries[0] as Entry;
  it("numbers turns per agent session, as the TUI does", () => {
    const entries: Entry[] = [
      entry,
      { ...entry, number: 3, kind: { kind: "between" } },
      { ...entry, number: 2 },
      { ...entry, number: 1, kind: { kind: "turn", session: ulid(200), agent: "Gemini CLI" } },
    ];
    expect(turnLabel({ entries }, entry)).toBe("Codex, turn 2");
    expect(turnLabel({ entries }, entries[1] as Entry)).toBe("Edits between turns");
    expect(turnLabel({ entries }, { ...entry, kind: { kind: "undo", of: 2 } })).toBe(
      "Put back as before #2",
    );
    expect(turnLabel({ entries }, { ...entry, kind: { kind: "moved", onto: "main" } })).toBe(
      "Moved onto main",
    );
  });
  it("reads added, deleted, quoted and binary paths once", () => {
    expect(
      filesInDiff(
        'diff --git a/a.ts b/a.ts\n--- /dev/null\n+++ b/a.ts\n@@ -0,0 +1 @@\n+new\ndiff --git a/old.ts b/old.ts\n--- a/old.ts\n+++ /dev/null\ndiff --git a/two b/two\n--- "a/two words.ts"\n+++ "b/two words.ts"\ndiff --git a/a.bin b/a.bin\nBinary files a/a.bin and b/a.bin differ',
      ),
    ).toEqual(["a.ts", "old.ts", "two words.ts", "a.bin"]);
  });
  it("does not turn deleted or added source text into a file header, and counts its lines", () => {
    const diff =
      "diff --git a/real b/real\n--- a/real\n+++ b/real\n@@ -1,2 +1,2 @@\n--- a/fake\n+++ b/fake\n-old\n+new";
    expect(filesInDiff(diff)).toEqual(["real"]);
    expect(turnFiles(diff)).toEqual([{ path: "real", lines: { added: 2, removed: 2 } }]);
    expect(
      filesInDiff("diff --git a/old b/new\nsimilarity index 100%\nrename from old\nrename to new"),
    ).toEqual(["new"]);
  });
  it("reads Git's UTF-8 octal paths", () => {
    expect(
      filesInDiff('diff --git a/a b/a\n--- "a/caf\\303\\251.txt"\n+++ "b/caf\\303\\251.txt"'),
    ).toEqual(["café.txt"]);
  });
});
