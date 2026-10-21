import { describe, expect, it } from "vitest";
import { defaultPreferences } from "../ipc/mock/scenario";
import { helpFor } from "./help";

describe("ARC-24 every key of the screen in view, as the TUI's help lists them", () => {
  it("heads each screen's keys as the TUI does", () => {
    const headings = (screen: Parameters<typeof helpFor>[0]) =>
      helpFor(screen, null).map((section) => section.heading);
    expect(headings("needs-you")).toEqual(["On the queue", "In a session", "Anywhere"]);
    expect(headings("all")).toEqual(["On the list", "In a session", "Anywhere"]);
    expect(headings("tickets")).toEqual(["On the tickets", "Choosing an agent", "Anywhere"]);
    expect(headings("settings")).toEqual(["On the settings", "On the plugins", "Anywhere"]);
  });

  it("lists the changes' keys, the person's, as the TUI's help over the changes does", () => {
    expect(helpFor("changes", null).map((section) => section.heading)).toEqual([
      "In the changes",
      "Anywhere",
    ]);
    const preferences = { ...defaultPreferences(), keys: { changes_again: ["g"] } };
    const changes = helpFor("changes", preferences)[0]?.entries ?? [];
    expect(changes).toContainEqual({ keys: "g", what: "ask git again" });
    expect(changes).toContainEqual({
      keys: "z",
      what: "on a turn, put the files back as before it, after asking",
    });
    expect(changes).toContainEqual({ keys: "d q", what: "back to the terminal" });
    expect(changes).toContainEqual({ keys: "?", what: "these keys" });
  });

  it("narrows to the keys whose keys or words hold what is typed, ignoring case", () => {
    expect(helpFor("all", null, "TIDY")).toEqual([
      {
        heading: "On the list",
        entries: [{ keys: "t", what: "have an agent tidy the worktree's commits" }],
      },
    ]);
    expect(helpFor("needs-you", null, "⌘K")[0]?.entries[0]?.what).toBe(
      "go to a session, a ticket or a setting, or do something",
    );
    expect(helpFor("needs-you", null, "nothing holds this")).toEqual([]);
  });

  it("shows the person's keys, and leaves out an action they took every key from", () => {
    const preferences = { ...defaultPreferences(), keys: { rename: ["ctrl-e"], close: [] } };
    const queue = helpFor("needs-you", preferences)[0]?.entries ?? [];
    expect(queue).toContainEqual({ keys: "⌃E", what: "rename the session's chat" });
    expect(queue.some((entry) => entry.what === "close the session")).toBe(false);
  });
});
