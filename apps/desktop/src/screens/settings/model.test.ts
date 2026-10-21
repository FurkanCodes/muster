import { describe, expect, it } from "vitest";
import {
  firstRow,
  keptRow,
  movedRow,
  projectOf,
  projectSection,
  rowKey,
  rowName,
  sectionOf,
  settingRows,
} from "./model";

const input = {
  agents: ["claude", "goose"],
  plugins: ["github", "zoe"],
  projects: [
    { id: "p1", loaded: true, fields: ["team"], prefix: true },
    { id: "p2", loaded: false, fields: [], prefix: false },
  ],
  keys: ["quit", "rename"],
};

describe("the settings as one list", () => {
  const rows = settingRows(input);
  const keys = rows.map((row) => row.key);

  it("goes through Muster's own sections, then each project's, in the boards' order", () => {
    expect(keys.slice(0, 5)).toEqual([
      "general/theme",
      "general/session_lines",
      "general/panels",
      "general/colours",
      "general/side_by_side",
    ]);
    expect(keys).toContain("notifications/tell.error");
    expect(keys).toContain("agents/agent.goose");
    expect(keys.indexOf("plugins/plugin.zoe")).toBe(keys.indexOf("plugins/add_plugin") - 1);
    expect(keys.filter((key) => key.startsWith("keys/"))).toEqual([
      "keys/find",
      "keys/key.quit",
      "keys/key.rename",
    ]);
    expect(keys.filter((key) => key.startsWith("project:p1/"))).toEqual([
      "project:p1/tickets_from",
      "project:p1/field.team",
      "project:p1/only_mine",
      "project:p1/muster_prefix",
      "project:p1/remove",
    ]);
    // A project whose settings are being read still has its way out.
    expect(keys.filter((key) => key.startsWith("project:p2/"))).toEqual(["project:p2/remove"]);
  });

  it("names a row by its page and its own part", () => {
    expect(rowKey("plugins", "plugin.zoe")).toBe("plugins/plugin.zoe");
    expect(sectionOf("project:p1/field.team")).toBe("project:p1");
    expect(rowName("project:p1/field.team")).toBe("field.team");
    expect(projectOf(projectSection("p1"))).toBe("p1");
    expect(projectOf("keys")).toBeNull();
  });

  it("moves through every page in order, no further than either end", () => {
    expect(movedRow(rows, "general/side_by_side", 1)?.key).toBe("notifications/notifications");
    expect(movedRow(rows, "notifications/notifications", -1)?.key).toBe("general/side_by_side");
    expect(movedRow(rows, "general/theme", -1)?.key).toBe("general/theme");
    expect(movedRow(rows, keys[keys.length - 1], 1)?.key).toBe("project:p2/remove");
    expect(movedRow(rows, "gone/row", 1)?.key).toBe("general/theme");
    expect(firstRow(rows, "naming")?.key).toBe("naming/names_by_agent");
  });

  it("keeps the selection on a row that exists", () => {
    expect(keptRow(rows, "naming/named_by")?.key).toBe("naming/named_by");
    expect(keptRow(rows, undefined)?.key).toBe("general/theme");
    // A plugin removed: the row that adds one.
    expect(keptRow(rows, "plugins/plugin.gone")?.key).toBe("plugins/add_plugin");
    // A key narrowed away: its page's first row.
    expect(keptRow(rows, "keys/key.close")?.key).toBe("keys/find");
    // A project removed: the first row of all.
    expect(keptRow(rows, "project:p9/only_mine")?.key).toBe("general/theme");
  });
});
