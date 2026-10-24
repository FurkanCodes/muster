// The layout read, written, completed, moved and sized, as
// `muster_core::layout`'s own tests have it (ARC-24: one grammar, both UIs).

import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT, Layout, LayoutError } from "./layout";

const read = (text: string) => Layout.parse(text);

describe("the layout's text", () => {
  it("reads and writes the default as it is written", () => {
    expect(Layout.default().toString()).toBe(DEFAULT_LAYOUT);
  });

  it("writes a layout as it reads", () => {
    for (const text of [
      "list | session | changes | shell | plugin",
      "list 30 | session 60% | (changes / shell / plugin)",
      "session | list | changes | (shell / plugin)",
      "(list / shell) 40% | session | (changes / plugin)",
      "list | (session / (changes | shell | plugin)) 70%",
    ]) {
      expect(read(text).toString()).toBe(text);
    }
    // Spaces and case are the person's; the layout writes its own way.
    expect(read("LIST|session|( changes/shell/Plugin )").toString()).toBe(
      "list | session | (changes / shell / plugin)",
    );
  });

  it("nests a run of one sign to the right", () => {
    const root = read("list | session | changes | shell | plugin").root;
    expect(root.part.kind).toBe("split");
    if (root.part.kind !== "split") return;
    expect(root.part.first.part).toEqual({ kind: "panel", panel: "list" });
    expect(root.part.second.part).toMatchObject({ kind: "split", split: "across" });
  });

  it("names what does not read", () => {
    const error = (text: string) => {
      try {
        Layout.parse(text);
      } catch (thrown) {
        expect(thrown).toBeInstanceOf(LayoutError);
        return (thrown as Error).message;
      }
      throw new Error(`${text} read`);
    };
    expect(error("list | list | session | (changes / shell)")).toBe("list is in the layout twice");
    expect(error("list | terminal | changes | shell")).toContain(
      "the panels are list, session, changes, shell and plugin",
    );
    for (const size of ["0%", "100%", "0", "6x"]) {
      expect(error(`list ${size} | session | changes | shell`)).toContain(
        `"${size}" is not a size`,
      );
    }
    expect(error("list | session / changes | shell")).toContain("are mixed");
    expect(error("(list | session | changes | shell")).toContain("bracket");
    expect(error("list | session | changes | shell)")).toContain("bracket");
    expect(error("list | | session | changes | shell")).toContain("missing");
    expect(error("list session | changes | shell")).toContain("missing");
  });

  it("gives a layout written before a panel the panel where the default puts it", () => {
    expect(read("list | session 60% | (changes / shell)").equals(Layout.default())).toBe(true);
    for (const [written, readAs] of [
      ["session | list 30 | (changes / shell)", "session | list 30 | (changes / shell / plugin)"],
      [
        "list 40 | session 60% | (changes / shell)",
        "list 40 | session 60% | (changes / shell / plugin)",
      ],
      ["(list / session) | (changes / shell)", "(list / session) | (changes / shell / plugin)"],
      ["list | (session / shell) 60% | changes", "list | (session / shell / plugin) 60% | changes"],
      [
        "(session 60% | (changes / shell)) | list",
        "(session 60% | (changes / shell / plugin)) | list",
      ],
      ["shell | list | session | changes", "(shell / plugin) | list | session | changes"],
      ["list | session | changes", "list | session | (changes / shell / plugin)"],
      ["session", "list | session | (changes / shell / plugin)"],
      ["plugin | session", "(changes / shell / plugin) | list | session"],
    ] as const) {
      expect(read(written).toString(), written).toBe(readAs);
    }
  });

  it("reads the default where a layout does not read", () => {
    expect(Layout.read("list | terminal").toString()).toBe(DEFAULT_LAYOUT);
    expect(Layout.read(null).toString()).toBe(DEFAULT_LAYOUT);
  });

  it("is given without a panel, as a client older than it is", () => {
    const older = Layout.default().without("plugin");
    expect(older.toString()).toBe("list | session 60% | (changes / shell)");
    expect(read(older.toString()).equals(Layout.default())).toBe(true);
  });
});

describe("moving a panel", () => {
  const layout = read(DEFAULT_LAYOUT);

  it("runs it along an edge of the window", () => {
    expect(layout.moved("list", { window: "right" }).toString()).toBe(
      "(session 60% | (changes / shell / plugin)) | list",
    );
    expect(layout.moved("shell", { window: "bottom" }).toString()).toBe(
      "(list | session 60% | (changes / plugin)) / shell",
    );
  });

  it("shares another's place beside it", () => {
    expect(layout.moved("shell", { beside: "session", edge: "bottom" }).toString()).toBe(
      "list | (session / shell) 60% | (changes / plugin)",
    );
    expect(layout.moved("changes", { beside: "list", edge: "top" }).toString()).toBe(
      "(changes / list) | session 60% | (shell / plugin)",
    );
  });

  it("keeps a width in cells but not a share", () => {
    const sized = read("list 30 | session 60% | (changes / shell / plugin)");
    expect(sized.moved("list", { window: "right" }).toString()).toBe(
      "(session 60% | (changes / shell / plugin)) | list 30",
    );
    expect(sized.moved("session", { beside: "shell", edge: "top" }).toString()).toBe(
      "list 30 | (changes / (session / shell) / plugin)",
    );
  });

  it("leaves a panel moved beside itself where it is", () => {
    expect(layout.moved("list", { beside: "list", edge: "right" })).toBe(layout);
  });

  it("puts a panel at an edge of the session's pane, the list staying where it is", () => {
    const pane = ["session", "changes", "shell", "plugin"] as const;
    expect(layout.movedToEdgeOf("shell", pane, "bottom").toString()).toBe(
      "list | ((session 60% | (changes / plugin)) / shell)",
    );
    expect(layout.movedToEdgeOf("plugin", pane, "left").toString()).toBe(
      "list | plugin | session 60% | (changes / shell)",
    );
    // In a window that is the pane, it is the window's edge.
    expect(layout.movedToEdgeOf("shell", [...pane, "list"], "bottom").toString()).toBe(
      layout.moved("shell", { window: "bottom" }).toString(),
    );
  });
});

describe("sizing a split", () => {
  const layout = read(DEFAULT_LAYOUT);

  it("sizes from either side", () => {
    expect(layout.sized([], "first", { cells: 28 }).toString()).toBe(
      "list 28 | session 60% | (changes / shell / plugin)",
    );
    expect(layout.sized(["second"], "second", { share: 30 }).toString()).toBe(
      "list | session | (changes / shell / plugin) 30%",
    );
    // A path that leads nowhere changes nothing.
    expect(layout.sized(["first"], "first", { share: 5 })).toBe(layout);
  });

  it("finds the split between panels shown past those not shown", () => {
    expect(layout.splitBetween(["list"], ["session"])).toEqual([]);
    expect(layout.splitBetween(["session"], ["shell"])).toEqual(["second"]);
    expect(layout.splitBetween(["changes"], ["shell"])).toEqual(["second", "second"]);
    expect(layout.splitBetween(["shell"], ["plugin"])).toEqual(["second", "second", "second"]);
    expect(layout.splitBetween(["shell"], ["changes"])).toBeNull();
  });
});

describe("the panels shown", () => {
  it("leave the place of those not shown to the other of their two", () => {
    const layout = read("list 30 | session 60% | (changes / shell / plugin)");
    const shown = layout.shown((panel) => panel === "list" || panel === "session");
    expect(shown).not.toBeNull();
    expect(shown?.part).toMatchObject({ kind: "split", split: "across" });
    if (shown?.part.kind !== "split") return;
    expect(shown.part.first).toEqual({
      part: { kind: "panel", panel: "list" },
      size: { cells: 30 },
    });
    expect(shown.part.second.part).toEqual({ kind: "panel", panel: "session" });
    expect(layout.shown(() => false)).toBeNull();
    expect(layout.shown((panel) => panel === "session")?.part).toEqual({
      kind: "panel",
      panel: "session",
    });
  });
});

describe("a session pinned in a panel of its own", () => {
  // As the wire has them; the layout's text prints them with `ses_`.
  const one = "01J0000000000000000000000A";
  const two = "01J0000000000000000000000B";

  it("reads, writes, moves and goes as any panel, as muster_core::layout has it", () => {
    const pinned = Layout.default().pinning(one, { beside: "session", edge: "left" });
    expect(pinned.toString()).toBe(
      `list | (session:ses_${one} | session) 60% | (changes / shell / plugin)`,
    );
    expect(read(pinned.toString()).toString()).toBe(pinned.toString());
    const both = pinned.pinning(two, { window: "bottom" });
    expect(both.pinned()).toEqual([one, two]);
    expect(() => read(`session:ses_${one} | ${pinned}`)).toThrow(LayoutError);
    const moved = both.pinning(one, { window: "right" });
    expect(moved.pinned()).toEqual([two, one]);
    expect(moved.unpinned(one).unpinned(two).toString()).toBe(DEFAULT_LAYOUT);
    expect(() => read("list | session:ses_nope | session | changes | shell | plugin")).toThrow(
      LayoutError,
    );
  });
});
