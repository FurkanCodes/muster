import { describe, expect, it } from "vitest";
import type { Clash } from "../../ipc/generated/Clash";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import {
  clashWords,
  commonFolder,
  entryLabel,
  oneLine,
  placeName,
  sameSummary,
  summaryOf,
} from "./labels";

const workspace = ulid(11);

function clash(files: [string, boolean][], more = 0): Clash {
  return {
    with: ulid(12),
    files: files.length + more,
    conflicts: files.filter(([, conflict]) => conflict).length,
    shown: files.map(([path, conflict]) => ({ path, conflict })),
  };
}

describe("ARC-24 the changes view's words", () => {
  it("names entries as the TUI does, with the board's comma", () => {
    const day = workingDay();
    const timeline = day.timelines[workspace] ?? { entries: [] };
    const at = (number: number) => timeline.entries.find((entry) => entry.number === number);
    const label = (number: number) => {
      const entry = at(number);
      return entry ? entryLabel(timeline, entry) : "";
    };
    // Entries 1, 2 and 4 are the same session's turns 1, 2 and 3.
    expect([label(4), label(3), label(2), label(1)]).toEqual([
      "Codex, turn 3",
      "Edits between turns",
      "Codex, turn 2",
      "Codex, turn 1",
    ]);
    const undo = { ...timeline.entries[0], kind: { kind: "undo", of: 2 } } as const;
    const moved = { ...timeline.entries[0], kind: { kind: "moved", onto: "main\nx" } } as const;
    expect(entryLabel(timeline, undo as never)).toBe("Put back as before #2");
    expect(entryLabel(timeline, moved as never)).toBe("Moved onto main x");
  });

  it("says a conflict where it is, and an overlap what both change, three named", () => {
    expect(
      clashWords(
        clash([
          ["src/a.rs", true],
          ["README.md", false],
        ]),
      ),
    ).toBe("would conflict in src/a.rs.");
    expect(
      clashWords(
        clash(
          [
            ["a", false],
            ["b", false],
            ["c", false],
            ["d", false],
          ],
          2,
        ),
      ),
    ).toBe("both change a, b, c and 3 more. Git could merge them.");
    expect(clashWords(clash([["two\nlines", true]]))).toBe("would conflict in two lines.");
  });

  it("names the other workspace by its kind and branch", () => {
    const day = workingDay();
    const find = (n: number) => day.workspaces.find((found) => found.id === ulid(n));
    expect(placeName(find(12))).toBe("⎇ fix/resize-under-load");
    expect(placeName(find(10))).toBe("⌂ main");
    expect(placeName(undefined)).toBe("another worktree");
  });

  it("keeps a line one line, as muster_core::text::one_line", () => {
    expect(oneLine("a\tb\r\nc\u0007d")).toBe("a b  cd");
  });

  it("finds the folder every path is in", () => {
    expect(commonFolder(["crates/desk/src/a.rs", "crates/desk/Cargo.toml"])).toBe("crates/desk/");
    expect(commonFolder(["crates/desk/a.rs", "crates/deskop/b.rs"])).toBe("crates/");
    expect(commonFolder(["src/a.rs", "Cargo.toml"])).toBe("");
    expect(commonFolder([])).toBe("");
  });

  it("sums a listing as the daemon's summary does", () => {
    const changes = workingDay().changes[workspace];
    if (!changes) throw new Error("no fixture");
    expect(summaryOf(changes)).toEqual({ files: 3, added: 300, removed: 6 });
    expect(
      summaryOf({ base: null, uncommitted: [], branch: [], commits: [], more_commits: 0 }),
    ).toBeNull();
    expect(
      sameSummary({ files: 1, added: 2, removed: 3 }, { files: 1, added: 2, removed: 3 }),
    ).toBe(true);
    expect(sameSummary(null, { files: 1, added: 0, removed: 0 })).toBe(false);
  });
});
