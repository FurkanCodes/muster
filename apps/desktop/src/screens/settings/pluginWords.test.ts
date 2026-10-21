import { describe, expect, it } from "vitest";
import { workingDayPlugins } from "../../ipc/mock/plugins";
import { defaultKeymap, keymapOf, panelsOf } from "../../keys/keymap";
import { keyEntries, narrowed } from "./keyRows";
import {
  brings,
  isFolder,
  missingPrograms,
  pluginName,
  pluginOrder,
  runs,
  runsOf,
  sourceText,
  wantsApproval,
} from "./pluginWords";

const plugins = workingDayPlugins();
const named = (id: string) => {
  const plugin = plugins.find((candidate) => candidate.id === id);
  if (!plugin) throw new Error(id);
  return plugin;
};

describe("a plugin, as Settings › Plugins says it", () => {
  it("is listed Muster's own first, then the person's by name", () => {
    const shuffled = [
      named("youtrack"),
      named("github"),
      named("session-graph"),
      named("notify-webhook"),
    ];
    expect(pluginOrder(shuffled).map((plugin) => plugin.id)).toEqual([
      "github",
      "notify-webhook",
      "session-graph",
      "youtrack",
    ]);
  });

  it("says where it is from", () => {
    expect(sourceText("builtin")).toBe("built into Muster");
    expect(sourceText(named("notify-webhook").source)).toBe(
      "the folder /Users/you/muster/examples/plugins/notify-webhook, read where it is",
    );
    expect(sourceText(named("youtrack").source)).toBe(
      "https://github.com/acme/muster-youtrack, main, commit 3f9a2c1",
    );
    expect(isFolder("/Users/you/x")).toBe(true);
    expect(isFolder("~/x")).toBe(true);
    expect(isFolder("git@github.com:you/x")).toBe(false);
  });

  it("says what it brings: panels with their keys as bound, tickets, the events its hooks hear", () => {
    const map = keymapOf({}, panelsOf(plugins));
    expect(brings(named("session-graph"), map)).toBe("panel: graph (⇧G)");
    expect(brings(named("github"), map)).toBe("tickets: GitHub");
    expect(brings(named("notify-webhook"), map)).toBe("hook: needs you, finished, failed");
    // The person's key for the panel, once set.
    const set = keymapOf({ "plugin.session-graph.graph": ["ctrl-g"] }, panelsOf(plugins));
    expect(brings(named("session-graph"), set)).toBe("panel: graph (⌃G)");
    // Off, it shows the key it suggests.
    expect(brings({ ...named("session-graph"), enabled: false }, defaultKeymap())).toBe(
      "panel: graph (⇧G)",
    );
  });

  it("lists everything it runs, word for word, and what it needs that is not found", () => {
    const youtrack = named("youtrack").details;
    if (!youtrack) throw new Error("details");
    expect(runsOf(youtrack)).toEqual([
      {
        what: "Tickets",
        command: "./youtrack --serve",
        how: "as a ticket source, YouTrack, asking for youtrack site; a query if you like",
      },
    ]);
    expect(missingPrograms(youtrack)).toEqual(["jq"]);
    const webhook = named("notify-webhook").details;
    if (!webhook) throw new Error("details");
    expect(runsOf(webhook)[0]).toEqual({
      what: "Hook notify",
      command: "./notify",
      how: "when an agent needs you, finishes or fails; at most 15 seconds each time",
    });
  });

  it("runs only while on and approved; one changed since wants approving again", () => {
    expect(runs(named("github"))).toBe(true);
    expect(runs(named("youtrack"))).toBe(false);
    expect(wantsApproval(named("youtrack"))).toBe(true);
    expect(wantsApproval({ ...named("youtrack"), problem: { Missing: { path: "/x" } } })).toBe(
      false,
    );
    expect(pluginName({ ...named("github"), details: null })).toBe("github");
  });
});

describe("Settings › Keys's rows", () => {
  const map = keymapOf({ rename: ["f2"] }, panelsOf(plugins));
  const entries = keyEntries(map);

  it("list every action under where it works, then the panels under their plugin", () => {
    expect(entries[0]).toMatchObject({
      name: "quit",
      group: "Anywhere",
      what: "Quit the UI (sessions keep running)",
    });
    expect(entries.find((entry) => entry.name === "rename")).toMatchObject({
      labels: ["F2"],
      set: true,
    });
    expect(entries[entries.length - 1]).toMatchObject({
      name: "plugin.session-graph.graph",
      group: "Session graph's panels",
      labels: ["⇧G"],
      set: false,
    });
  });

  it("narrow to what is typed, by words, name or key, ignoring case", () => {
    expect(narrowed(entries, "").length).toBe(entries.length);
    expect(narrowed(entries, "PULL REQUEST").map((entry) => entry.name)).toEqual([
      "open_pull_request",
      "pull_request_in_browser",
    ]);
    expect(narrowed(entries, "f2").map((entry) => entry.name)).toEqual(["rename"]);
    expect(narrowed(entries, "graph").map((entry) => entry.name)).toEqual([
      "plugin.session-graph.graph",
    ]);
  });
});
