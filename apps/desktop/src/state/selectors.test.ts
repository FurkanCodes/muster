import { describe, expect, it } from "vitest";
import { shownHarness, standsBeside } from "./selectors";

const at = 1_700_000_000_000;

describe("a terminal with an agent in front (ARC-28)", () => {
  it("shows as that agent, and is a chat while it runs", () => {
    const terminal = { harness: "shell", agent: null };
    expect(shownHarness(terminal)).toBe("shell");
    expect(standsBeside(terminal)).toBe(true);
    const running = { harness: "shell", agent: { harness: "codex", since: at } };
    expect(shownHarness(running)).toBe("codex");
    expect(standsBeside(running)).toBe(false);
  });

  it("leaves an agent's own session and a panel as they are", () => {
    expect(shownHarness({ harness: "claude", agent: null })).toBe("claude");
    expect(standsBeside({ harness: "plugin:zoe/graph", agent: null })).toBe(true);
  });
});
