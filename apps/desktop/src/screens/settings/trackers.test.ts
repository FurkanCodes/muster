import { describe, expect, it } from "vitest";
import { workingDayPlugins } from "../../ipc/mock/plugins";
import type { Read } from "./context";
import { chosenTracker, fieldsFor, missingField, offeredTrackers, ticketsFrom } from "./trackers";

function settings(tracker: string | null, fields: Record<string, string> = {}): Read {
  return {
    state: "loaded",
    settings: {
      tickets_from: "",
      only_mine: { on: false, from: "default" },
      file: "f",
      tickets: tracker ? { tracker, fields } : null,
      tickets_set_by: tracker ? "yours" : "default",
      tickets_untold: false,
      muster_prefix: null,
    },
  };
}

describe("where a project's tickets can come from", () => {
  const plugins = workingDayPlugins();
  const offered = offeredTrackers(plugins);

  it("offers the ticket plugins that run: Muster's own in the daemon's order, then anyone's by name", () => {
    expect(offered.map((one) => one.id)).toEqual(["github", "gitlab", "jira", "linear"]);
    const theirs = plugins.map((plugin) =>
      plugin.id === "youtrack" ? { ...plugin, problem: null } : plugin,
    );
    expect(offeredTrackers(theirs).map((one) => one.name)).toEqual([
      "GitHub",
      "GitLab",
      "Jira",
      "Linear",
      "YouTrack",
    ]);
  });

  it("fills a tracker's fields with what was typed, else what is written, else another project's site", () => {
    const projects = new Map<string, Read>([
      ["a", settings(null)],
      ["b", settings("jira", { site: "acme.atlassian.net", project: "APP" })],
    ]);
    const jira = fieldsFor(offered, projects, "a", "jira", undefined);
    expect(jira.map((field) => [field.setting.key, field.text])).toEqual([
      ["site", "acme.atlassian.net"],
      ["project", ""],
      ["jql", ""],
    ]);
    expect(missingField(jira)).toBeUndefined();
    const typed = fieldsFor(offered, projects, "a", "jira", {
      tracker: "jira",
      typed: { jira: { site: "other.atlassian.net" } },
    });
    expect(typed[0]?.text).toBe("other.atlassian.net");
    const linear = fieldsFor(offered, projects, "a", "linear", undefined);
    expect(missingField(linear)?.setting.key).toBe("team");
    expect(fieldsFor(offered, projects, "a", null, undefined)).toEqual([]);
  });

  it("is chosen as it is being chosen, else as written", () => {
    expect(chosenTracker(settings("linear"), undefined)).toBe("linear");
    expect(chosenTracker(settings("linear"), { tracker: null, typed: {} })).toBeNull();
    expect(chosenTracker(undefined, undefined)).toBeNull();
  });

  it("is written whole, the empty left out, or not at all to follow the git remote", () => {
    const projects = new Map<string, Read>([["a", settings(null)]]);
    const fields = fieldsFor(offered, projects, "a", "jira", {
      tracker: "jira",
      typed: { jira: { site: " acme.atlassian.net ", project: "" } },
    });
    expect(ticketsFrom("jira", fields)).toEqual({
      tracker: "jira",
      fields: { site: "acme.atlassian.net" },
    });
    expect(ticketsFrom(null, [])).toBeNull();
  });
});
