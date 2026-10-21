import { describe, expect, it } from "vitest";
import type { PluginInfo } from "../ipc/generated/PluginInfo";
import { parseChord } from "./chord";
import {
  ACTIONS,
  actionAt,
  checkKey,
  defaultKeymap,
  isPanelName,
  isSet,
  keymapOf,
  labelOf,
  labelsOf,
  type PanelAction,
  panelName,
  panelsOf,
  refusalText,
  screenKeys,
  warningsOf,
  warningText,
} from "./keymap";

function chord(text: string) {
  const parsed = parseChord(text);
  if (!parsed) throw new Error(`${text} is no key`);
  return parsed;
}

function zoetrope(panel: string, suggested: string | null): PanelAction {
  return { plugin: "zoetrope", panel, pluginName: "Zoetrope", title: panel, suggested };
}

describe("Muster's own keys", () => {
  it("never clash, each action named once", () => {
    expect(keymapOf({}).problem).toBeNull();
    expect(new Set(ACTIONS.map((action) => action.name)).size).toBe(ACTIONS.length);
  });

  it("do what their screen says", () => {
    const keys = defaultKeymap();
    expect(actionAt(keys, "sessions", chord("r"))).toBe("restart_session");
    expect(actionAt(keys, "tickets", chord("r"))).toBe("refresh_tickets");
    expect(actionAt(keys, "changes", chord("r"))).toBe("changes_again");
    expect(actionAt(keys, "settings", chord("r"))).toBe("read_settings");
    expect(actionAt(keys, "in_session", chord("r"))).toBeNull();
    expect(actionAt(keys, "in_session", chord("Q"))).toBe("leave_session");
    expect(actionAt(keys, "sessions", chord("Q"))).toBeNull();
    expect(labelOf(keys, "rename")).toBe("⇧R");
    expect(labelsOf(keys, "help")).toEqual(["h", "?"]);
  });

  it("are named for the window's listener on each screen", () => {
    const settings = screenKeys(defaultKeymap(), "settings");
    expect(settings.get("r")).toBe("read_settings");
    expect(settings.get("a")).toBe("add_plugin");
    expect(settings.get("Escape")).toBe("back");
    expect(screenKeys(defaultKeymap(), "in_session").get("⌃]")).toBe("leave_session");
  });
});

describe("the person's keys", () => {
  it("take the place of Muster's", () => {
    const keys = keymapOf({ new_session: ["N"], rename: ["f2"] });
    expect(actionAt(keys, "sessions", chord("N"))).toBe("new_session");
    expect(actionAt(keys, "sessions", chord("n"))).toBeNull();
    expect(labelOf(keys, "rename")).toBe("F2");
    expect(isSet(keys, "rename")).toBe(true);
    expect(isSet(keys, "close")).toBe(false);
    expect(labelOf(keymapOf({ move_panels: [] }), "move_panels")).toBe("none");
  });

  it("that do not read leave Muster's, saying why", () => {
    const keys = keymapOf({ new_session: ["d"] });
    expect(keys.problem).toBe("keys.new_session: d is show_changes's on the same screen");
    expect(labelOf(keys, "new_session")).toBe("n");
  });

  it("kept aside under a name that is no action are warned of, never refused", () => {
    const keys = keymapOf({ zoetrope_graph: ["Z"], open_sesame: ["not a key"], rename: ["f2"] });
    expect(keys.problem).toBeNull();
    expect(labelOf(keys, "rename")).toBe("F2");
    expect(actionAt(keys, "sessions", chord("Z"))).toBeNull();
    expect(warningsOf(keys).map(warningText)).toEqual([
      "keys.open_sesame is not something a key does in Muster: kept, for a plugin's action of that name, and doing nothing meanwhile",
      "keys.zoetrope_graph is not something a key does in Muster: kept, for a plugin's action of that name, and doing nothing meanwhile",
    ]);
    expect(warningsOf(defaultKeymap())).toEqual([]);
  });
});

describe("setting a key", () => {
  it("refuses one another action has on a screen they share, naming it", () => {
    const keys = defaultKeymap();
    const refused = checkKey(keys, "new_session", chord("d"));
    expect(refused).toEqual({ kind: "taken", key: "d", other: "show_changes" });
    expect(refused && refusalText(keys, refused)).toBe(
      "d already does “What the worktree or session has changed, and its turns” on this screen. Press another key.",
    );
    // On screens they never share, two actions may have one key.
    expect(checkKey(keys, "refresh_tickets", chord("n"))).toBeNull();
    // Its own key is no clash.
    expect(checkKey(keys, "new_session", chord("n"))).toBeNull();
  });

  it("refuses one every screen keeps", () => {
    for (const key of ["enter", "up", "ctrl-c", "pagedown", "home"]) {
      expect(checkKey(defaultKeymap(), "quit", chord(key))?.kind, key).toBe("kept");
    }
    const refused = checkKey(defaultKeymap(), "quit", chord("enter"));
    expect(refused && refusalText(defaultKeymap(), refused)).toBe(
      "⏎ is ⏎, which does what is selected, the same on every screen. Press another key.",
    );
  });

  it("refuses, inside a terminal, one its program could take as typed", () => {
    const refused = checkKey(defaultKeymap(), "leave_session", chord("x"));
    expect(refused).toEqual({ kind: "typed", key: "x" });
    expect(refused && refusalText(defaultKeymap(), refused)).toContain("needs ⌃ or ⌥");
    expect(checkKey(defaultKeymap(), "leave_session", chord("alt-q"))).toBeNull();
    expect(checkKey(defaultKeymap(), "fold_list", chord("Z"))).toBeNull();
  });
});

describe("plugins' panels", () => {
  it("are named for their plugin and panel", () => {
    expect(panelName("zoetrope", "graph")).toBe("plugin.zoetrope.graph");
    expect(isPanelName("plugin.zoetrope.graph")).toBe(true);
    expect(isPanelName("plugin.my-panel.files-2")).toBe(true);
    for (const not of [
      "rename",
      "plugin.zoetrope",
      "plugin..graph",
      "plugin.Zoe.graph",
      "plugin.a.b.c",
      "zoetrope_graph",
    ]) {
      expect(isPanelName(not), not).toBe(false);
    }
  });

  it("have the key they suggest on the lists of sessions only", () => {
    const keys = keymapOf({}, [zoetrope("graph", "Z")]);
    expect(actionAt(keys, "sessions", chord("Z"))).toBe("plugin.zoetrope.graph");
    expect(actionAt(keys, "needs_you", chord("Z"))).toBe("plugin.zoetrope.graph");
    expect(actionAt(keys, "in_session", chord("Z"))).toBeNull();
    expect(actionAt(keys, "settings", chord("Z"))).toBeNull();
    expect(labelOf(keys, "plugin.zoetrope.graph")).toBe("⇧Z");
    expect(isSet(keys, "plugin.zoetrope.graph")).toBe(false);
    expect(warningsOf(keys)).toEqual([]);
  });

  it("drop a suggestion Muster has a use for, and warn of it", () => {
    const keys = keymapOf({}, [
      zoetrope("graph", "T"),
      zoetrope("files", "enter"),
      zoetrope("log", "G"),
      zoetrope("again", "G"),
    ]);
    expect(labelOf(keys, "plugin.zoetrope.graph")).toBe("none");
    expect(labelOf(keys, "plugin.zoetrope.files")).toBe("none");
    expect(labelOf(keys, "plugin.zoetrope.log")).toBe("⇧G");
    expect(labelOf(keys, "plugin.zoetrope.again")).toBe("none");
    const warnings = warningsOf(keys).map(warningText);
    expect(warnings).toHaveLength(3);
    expect(warnings[0]).toMatch(
      /^the Zoetrope plugin's graph panel suggests T, which is Muster's for shell/,
    );
    expect(warnings[2]).toContain("plugin.zoetrope.log's");
    expect(actionAt(keys, "sessions", chord("T"))).toBe("shell");
  });

  it("take the person's key over their suggestion", () => {
    const keys = keymapOf({ "plugin.zoetrope.graph": ["G"], "plugin.gone.panel": ["Y"] }, [
      zoetrope("graph", "Z"),
    ]);
    expect(labelsOf(keys, "plugin.zoetrope.graph")).toEqual(["⇧G"]);
    expect(isSet(keys, "plugin.zoetrope.graph")).toBe(true);
    expect(actionAt(keys, "sessions", chord("Z"))).toBeNull();
    expect(warningsOf(keys)).toEqual([{ kind: "not_an_action", name: "plugin.gone.panel" }]);
    const taken = keymapOf({ "plugin.zoetrope.graph": ["d"] }, [zoetrope("graph", "Z")]);
    expect(labelOf(taken, "plugin.zoetrope.graph")).toBe("none");
    expect(warningText(warningsOf(taken)[0] as never)).toMatch(
      /^keys\.plugin\.zoetrope\.graph: d is Muster's for show_changes/,
    );
  });

  it("never let a suggestion take a key the person gave another panel", () => {
    const first = { ...zoetrope("files", "G"), plugin: "aaa" };
    const keys = keymapOf({ "plugin.zoetrope.graph": ["G"] }, [first, zoetrope("graph", "Z")]);
    expect(labelOf(keys, "plugin.zoetrope.graph")).toBe("⇧G");
    expect(labelOf(keys, "plugin.aaa.files")).toBe("none");
    expect(keys.panels.map((panel) => panel.name)).toEqual([
      "plugin.aaa.files",
      "plugin.zoetrope.graph",
    ]);
    expect(warningText(warningsOf(keys)[0] as never)).toContain(
      "suggests G, which is plugin.zoetrope.graph's",
    );
  });

  it("are checked against Muster's keys and each other's as the person sets one", () => {
    const keys = keymapOf({}, [zoetrope("graph", "Z"), zoetrope("files", "G")]);
    const graph = "plugin.zoetrope.graph";
    expect(checkKey(keys, graph, chord("Y"))).toBeNull();
    expect(checkKey(keys, graph, chord("Z"))).toBeNull();
    expect(checkKey(keys, graph, chord("G"))).toMatchObject({ other: "plugin.zoetrope.files" });
    expect(checkKey(keys, graph, chord("s"))).toMatchObject({ other: "stop_session" });
    expect(checkKey(keys, graph, chord("up"))?.kind).toBe("kept");
    expect(checkKey(keys, "shell", chord("Z"))).toMatchObject({ other: graph });
    expect(checkKey(keys, "read_settings", chord("Z"))).toBeNull();
    const refused = checkKey(keys, "shell", chord("Z"));
    expect(refused && refusalText(keys, refused)).toBe(
      "⇧Z already does “Graph beside the session (again: put it away)” on this screen. Press another key.",
    );
  });

  it("come from the plugins that run, in the daemon's order", () => {
    const plugin = (id: string, enabled: boolean, key: string | null): PluginInfo => ({
      id,
      source: "builtin",
      enabled,
      problem: null,
      changed: null,
      details: {
        name: id.toUpperCase(),
        version: "1",
        description: null,
        muster: "0.4.0",
        requires: [],
        how_to_get: null,
        env: [],
        secrets: [],
        panels: [{ id: "graph", title: "graph", on: "session", command: ["zoe"], key }],
        tickets: null,
        hooks: [],
        sha256: "ab",
      },
    });
    expect(panelsOf([plugin("one", true, "G"), plugin("two", false, "Y")])).toEqual([
      { plugin: "one", panel: "graph", pluginName: "ONE", title: "graph", suggested: "G" },
    ]);
  });
});
