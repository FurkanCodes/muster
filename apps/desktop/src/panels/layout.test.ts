// The layout read, written, completed, moved, sized and swapped, as
// `muster_core::layout`'s own tests have it (ARC-24: one grammar, both UIs).

import { describe, expect, it } from "vitest";
import { DEFAULT_LAYOUT, Layout, LayoutError, placedPanel } from "./layout";

const read = (text: string) => Layout.parse(text);

// As the wire has them; the layout's text prints them with `ses_`.
const one = "01J0000000000000000000000A";
const two = "01J0000000000000000000000B";

describe("the layout's text", () => {
  it("reads and writes the default as it is written", () => {
    expect(Layout.default().toString()).toBe(DEFAULT_LAYOUT);
  });

  it("writes a layout as it reads", () => {
    for (const text of [
      "list | session | changes | plugin",
      "list 30 | session 60% | (changes / plugin)",
      "session | list | (changes / plugin)",
      `(list / session:ses_${one}) 40% | session | (changes / plugin)`,
      `list | (session / (changes | session:ses_${one} | plugin)) 70%`,
    ]) {
      expect(read(text).toString()).toBe(text);
    }
    // Spaces and case are the person's; the layout writes its own way.
    expect(read("LIST|session|( changes/Plugin )").toString()).toBe(
      "list | session | (changes / plugin)",
    );
  });

  it("nests a run of one sign to the right", () => {
    const root = read("list | session | changes | plugin").root;
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
    expect(error("list | list | session | (changes / plugin)")).toBe("list is in the layout twice");
    expect(error("list | terminal | changes | plugin")).toContain(
      "the panels are list, session, changes and plugin",
    );
    for (const size of ["0%", "100%", "0", "6x"]) {
      expect(error(`list ${size} | session | changes | plugin`)).toContain(
        `"${size}" is not a size`,
      );
    }
    expect(error("list | session / changes | plugin")).toContain("are mixed");
    expect(error("(list | session | changes | plugin")).toContain("bracket");
    expect(error("list | session | changes | plugin)")).toContain("bracket");
    expect(error("list | | session | changes | plugin")).toContain("missing");
    expect(error("list session | changes | plugin")).toContain("missing");
  });

  it("gives a layout written before a panel the panel where the default puts it", () => {
    for (const [written, readAs] of [
      ["session | list 30 | changes", "session | list 30 | (changes / plugin)"],
      ["list 40 | session 60% | changes", "list 40 | session 60% | (changes / plugin)"],
      ["(list / session) | changes", "(list / session) | (changes / plugin)"],
      ["(session 60% | changes) | list", "(session 60% | (changes / plugin)) | list"],
      ["list | session", "list | session | (changes / plugin)"],
      ["session", "list | session | (changes / plugin)"],
      ["plugin | session", "(changes / plugin) | list | session"],
    ] as const) {
      expect(read(written).toString(), written).toBe(readAs);
    }
  });

  it("reads a layout naming the retired shell panel without it", () => {
    for (const [written, readAs] of [
      ["list | session 60% | (changes / shell / plugin)", DEFAULT_LAYOUT],
      ["list | session 60% | (changes / shell)", DEFAULT_LAYOUT],
      [
        "list 30 | (session / shell) 70% | (changes / plugin)",
        "list 30 | session 70% | (changes / plugin)",
      ],
      ["shell | list | session | (changes / plugin)", "list | session | (changes / plugin)"],
      [
        `session:ses_${one} | list | session 60% | (changes / Shell / plugin)`,
        `session:ses_${one} | list | session 60% | (changes / plugin)`,
      ],
    ] as const) {
      expect(read(written).toString(), written).toBe(readAs);
    }
    expect(read("shell").toString()).toBe(DEFAULT_LAYOUT);
  });

  it("reads the default where a layout does not read", () => {
    expect(Layout.read("list | terminal").toString()).toBe(DEFAULT_LAYOUT);
    expect(Layout.read(null).toString()).toBe(DEFAULT_LAYOUT);
  });

  it("is given without a panel, as a client older than it is", () => {
    const older = Layout.default().without("plugin");
    expect(older.toString()).toBe("list | session 60% | changes");
    expect(read(older.toString()).equals(Layout.default())).toBe(true);
  });
});

describe("moving a panel", () => {
  const layout = read(DEFAULT_LAYOUT);

  it("runs it along an edge of the window", () => {
    expect(layout.moved("list", { window: "right" }).toString()).toBe(
      "(session 60% | (changes / plugin)) | list",
    );
    expect(layout.moved("plugin", { window: "bottom" }).toString()).toBe(
      "(list | session 60% | changes) / plugin",
    );
  });

  it("shares another's place beside it", () => {
    expect(layout.moved("plugin", { beside: "session", edge: "bottom" }).toString()).toBe(
      "list | (session / plugin) 60% | changes",
    );
    expect(layout.moved("changes", { beside: "list", edge: "top" }).toString()).toBe(
      "(changes / list) | session 60% | plugin",
    );
  });

  it("keeps a width in cells but not a share", () => {
    const sized = read("list 30 | session 60% | (changes / plugin)");
    expect(sized.moved("list", { window: "right" }).toString()).toBe(
      "(session 60% | (changes / plugin)) | list 30",
    );
    expect(sized.moved("session", { beside: "plugin", edge: "top" }).toString()).toBe(
      "list 30 | (changes / session / plugin)",
    );
  });

  it("leaves a panel moved beside itself where it is", () => {
    expect(layout.moved("list", { beside: "list", edge: "right" })).toBe(layout);
  });

  it("puts a panel at an edge of the session's pane, the list staying where it is", () => {
    const pane = ["session", "changes", "plugin"] as const;
    expect(layout.movedToEdgeOf("plugin", pane, "bottom").toString()).toBe(
      "list | ((session 60% | changes) / plugin)",
    );
    expect(layout.movedToEdgeOf("plugin", pane, "left").toString()).toBe(
      "list | plugin | session 60% | changes",
    );
    // In a window that is the pane, it is the window's edge.
    expect(layout.movedToEdgeOf("plugin", [...pane, "list"], "bottom").toString()).toBe(
      layout.moved("plugin", { window: "bottom" }).toString(),
    );
  });
});

describe("swapping two panels", () => {
  it("puts each in the other's place, each place keeping its share", () => {
    const layout = read(`list | session 60% | (session:ses_${one} / plugin) | changes`);
    const swapped = layout.swapped("session", placedPanel(one));
    expect(swapped.toString()).toBe(`list | session:ses_${one} 60% | (session / plugin) | changes`);
    expect(swapped.swapped(placedPanel(one), "session").equals(layout)).toBe(true);
    expect(layout.swapped("list", "changes").toString()).toBe(
      `changes | session 60% | (session:ses_${one} / plugin) | list`,
    );
  });

  it("carries a width in cells with its panel", () => {
    const layout = read("list 30 | session 60% | (changes / plugin)");
    expect(layout.swapped("list", "session").toString()).toBe(
      "session | list 30 | (changes / plugin)",
    );
  });

  it("is what a panel dropped on another's middle does, and leaves one not there", () => {
    const layout = Layout.default().placing(one, { beside: "session", edge: "right" });
    expect(layout.moved(placedPanel(one), { over: "list" }).toString()).toBe(
      layout.swapped(placedPanel(one), "list").toString(),
    );
    expect(layout.swapped("plugin", "plugin")).toBe(layout);
    expect(layout.swapped("plugin", placedPanel(two))).toBe(layout);
    // A session not placed yet has no place to give: it goes beside.
    expect(layout.placing(two, { over: "changes" }).toString()).toBe(
      layout.placing(two, { beside: "changes", edge: "right" }).toString(),
    );
  });
});

describe("sizing a split", () => {
  const layout = read(DEFAULT_LAYOUT);

  it("sizes from either side", () => {
    expect(layout.sized([], "first", { cells: 28 }).toString()).toBe(
      "list 28 | session 60% | (changes / plugin)",
    );
    expect(layout.sized(["second"], "second", { share: 30 }).toString()).toBe(
      "list | session | (changes / plugin) 30%",
    );
    // A path that leads nowhere changes nothing.
    expect(layout.sized(["first"], "first", { share: 5 })).toBe(layout);
  });

  it("finds the split between panels shown past those not shown", () => {
    const placed = read(`list | session 60% | (changes / session:ses_${one} / plugin)`);
    const panel = placedPanel(one);
    expect(placed.splitBetween(["list"], ["session"])).toEqual([]);
    expect(placed.splitBetween(["session"], [panel])).toEqual(["second"]);
    expect(placed.splitBetween(["changes"], [panel])).toEqual(["second", "second"]);
    expect(placed.splitBetween([panel], ["plugin"])).toEqual(["second", "second", "second"]);
    expect(placed.splitBetween([panel], ["changes"])).toBeNull();
  });
});

describe("the panels shown", () => {
  it("leave the place of those not shown to the other of their two", () => {
    const layout = read("list 30 | session 60% | (changes / plugin)");
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

describe("a session placed in a panel of its own", () => {
  it("reads, writes, moves and goes as any panel, as muster_core::layout has it", () => {
    const placed = Layout.default().placing(one, { beside: "session", edge: "left" });
    expect(placed.toString()).toBe(
      `list | (session:ses_${one} | session) 60% | (changes / plugin)`,
    );
    expect(read(placed.toString()).toString()).toBe(placed.toString());
    const both = placed.placing(two, { window: "bottom" });
    expect(both.placed()).toEqual([one, two]);
    expect(() => read(`session:ses_${one} | ${placed}`)).toThrow(LayoutError);
    const moved = both.placing(one, { window: "right" });
    expect(moved.placed()).toEqual([two, one]);
    expect(moved.unplaced(one).unplaced(two).toString()).toBe(DEFAULT_LAYOUT);
    expect(() => read("list | session:ses_nope | session | changes | plugin")).toThrow(LayoutError);
  });
});
