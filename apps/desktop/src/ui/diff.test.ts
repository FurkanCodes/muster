import { describe, expect, it } from "vitest";
import { lineAt, parseDiff } from "./diff";

describe("ARC-24 unified diffs", () => {
  it("numbers both sides, resets at hunks and files, and expands tabs", () => {
    const lines = parseDiff(
      "--- a/a\n+++ b/a\n@@ -8,2 +10,3 @@\n old\n-gone\n+new\n+\ttab\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-x\n+y\n",
    );
    expect(lines.slice(3, 7).map(({ kind, old, next, text }) => [kind, old, next, text])).toEqual([
      ["context", 8, 10, "old"],
      ["del", 9, null, "gone"],
      ["add", null, 11, "new"],
      ["add", null, 12, "    tab"],
    ]);
    expect(lines[9]?.kind).toBe("header");
    expect(lines.at(-1)?.next).toBe(1);
    expect(lineAt(lines, lines[4]?.offset ?? 0)).toBe(3);
  });
  it("ARC-25 measures parsing 5,000 lines within an interactive budget", () => {
    const source = `@@ -0,0 +1,5000 @@\n${Array.from({ length: 5000 }, (_, n) => `+line ${n}`).join("\n")}`;
    const start = performance.now();
    const lines = parseDiff(source);
    const elapsed = performance.now() - start;
    expect(lines).toHaveLength(5001);
    expect(lines.at(-1)?.next).toBe(5000);
    expect(elapsed).toBeLessThan(100);
  });
});
