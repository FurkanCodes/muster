// The panels laid out over the pane, as the terminal UI's `layout::solve`
// lays them over its window, in pixels (ARC-24, PRD §6).

import { describe, expect, it } from "vitest";
import { Layout } from "./layout";
import { GAP, landingAt, needs, PANEL_LEAST, shareAt, solve } from "./solve";

const PANE = { x: 0, y: 0, width: 1008, height: 800 };
const shown = (text: string, panels: string[]) => {
  const node = Layout.parse(text).shown((panel) => panels.includes(panel));
  if (!node) throw new Error("nothing shown");
  return node;
};

describe("laying out the panels", () => {
  it("shares a place in half where neither part is sized", () => {
    const solved = solve(
      shown("list | session | (changes / shell / plugin)", ["session", "shell"]),
      PANE,
    );
    expect(solved.panels).toEqual([
      { panel: "session", rect: { x: 0, y: 0, width: 500, height: 800 } },
      { panel: "shell", rect: { x: 508, y: 0, width: 500, height: 800 } },
    ]);
    expect(solved.dividers).toHaveLength(1);
    expect(solved.dividers[0]?.line).toEqual({ x: 500, y: 0, width: GAP, height: 800 });
    expect(solved.dividers[0]?.first).toEqual(["session"]);
  });

  it("gives a share or cells to the part sized, the other the rest", () => {
    const shared = solve(
      shown("list | session 60% | (changes / shell)", ["session", "shell"]),
      PANE,
    );
    expect(shared.panels[0]?.rect.width).toBe(600);
    const cells = solve(shown("list | (session / shell 10)", ["session", "shell"]), PANE);
    expect(cells.panels[1]?.rect).toEqual({
      x: 0,
      y: 800 - GAP - 200 + GAP,
      width: 1008,
      height: 200,
    });
  });

  it("keeps each part at least what a panel needs", () => {
    const solved = solve(shown("list | session 90% | shell", ["session", "shell"]), PANE);
    expect(solved.panels[1]?.rect.width).toBe(PANEL_LEAST.width);
    expect(needs(shown(Layout.default().toString(), ["session", "shell", "plugin"]))).toEqual({
      width: PANEL_LEAST.width * 2 + GAP,
      height: PANEL_LEAST.height * 2 + GAP,
    });
  });

  it("lays three out as the default does: the shell above the plugin's panel, beside the agent", () => {
    const solved = solve(shown(Layout.default().toString(), ["session", "shell", "plugin"]), PANE);
    const at = Object.fromEntries(solved.panels.map(({ panel, rect }) => [panel, rect]));
    expect(at.session?.x).toBe(0);
    expect(at.shell?.x).toBe(at.plugin?.x);
    expect(at.shell?.y).toBe(0);
    expect(at.plugin?.y).toBeGreaterThan(0);
  });
});

describe("dragging", () => {
  const solved = solve(shown("session | shell", ["session", "shell"]), PANE);

  it("shares a line's place as far as the pointer, between 10% and 90%", () => {
    const line = solved.dividers[0];
    if (!line) throw new Error("no line");
    expect(shareAt(line, 300)).toBe(30);
    expect(shareAt(line, 2)).toBe(10);
    expect(shareAt(line, 1000)).toBe(90);
  });

  it("lands a panel at the pane's edge near it, or the nearest edge of the panel under it", () => {
    expect(landingAt("shell", 5, 400, PANE, solved.panels)).toMatchObject({
      edge: "left",
      target: null,
    });
    expect(landingAt("shell", 500, 795, PANE, solved.panels)).toMatchObject({
      edge: "bottom",
      target: null,
    });
    expect(landingAt("shell", 200, 700, PANE, solved.panels)).toMatchObject({
      edge: "bottom",
      target: "session",
      rect: { x: 0, y: 400, width: 500, height: 400 },
    });
    expect(landingAt("shell", 60, 400, PANE, solved.panels)).toMatchObject({
      edge: "left",
      target: "session",
    });
    // Over itself, nowhere.
    expect(landingAt("shell", 700, 400, PANE, solved.panels)).toBeNull();
  });
});
