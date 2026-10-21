import { describe, expect, it } from "vitest";
import type { Folder } from "../ipc/generated/desktop/Folder";
import { root, shown, split, step, up } from "./folders";
import { found, narrowed, rank } from "./match";

const folder = (name: string, git = false): Folder => ({
  name,
  path: `/Users/you/code/${name}`,
  git,
});
const code = [".cache", "kep-api", "labs", "lamphaus", "lamphaus-api", "Solar"].map((n) =>
  folder(n),
);

describe("PRD-01 the folder browser, as the TUI's picker", () => {
  it("splits a typed path into its folder and the name after the last /", () => {
    expect(split("~/code/la")).toEqual(["~/code/", "la"]);
    expect(split("~/")).toEqual(["~/", ""]);
    expect(split("code")).toEqual(["", "code"]);
  });

  it("narrows by the name, ignoring case, those starting with it first; dots only once typed", () => {
    expect(shown(code, "").map((f) => f.name)).toEqual([
      "kep-api",
      "labs",
      "lamphaus",
      "lamphaus-api",
      "Solar",
    ]);
    expect(shown(code, "A").map((f) => f.name)).toEqual([
      "kep-api",
      "labs",
      "lamphaus",
      "lamphaus-api",
      "Solar",
    ]);
    expect(shown(code, "api").map((f) => f.name)).toEqual(["kep-api", "lamphaus-api"]);
    expect(shown(code, "s").map((f) => f.name)).toEqual([
      "Solar",
      "labs",
      "lamphaus",
      "lamphaus-api",
    ]);
    expect(shown(code, ".").map((f) => f.name)).toEqual([".cache"]);
  });

  it("steps the choice: down from none is the first, up from the first is the path as typed", () => {
    expect(step(null, 1, 3)).toBe(0);
    expect(step(0, -1, 3)).toBeNull();
    expect(step(1, 8, 3)).toBe(2);
    expect(step(null, -1, 3)).toBeNull();
    expect(step(0, 1, 0)).toBeNull();
  });

  it("goes up a folder, from ~ through where it was listed", () => {
    expect(up("~/code/lamphaus/", "/Users/you/code/lamphaus")).toBe("~/code/");
    expect(up("~/", "/Users/you")).toBe("/Users/");
    expect(up("/Users/", "/Users")).toBe("/");
    expect(up("/", "/")).toBe("/");
  });

  it("adds the chosen folder, or the path as typed with ~ as listed", () => {
    expect(root("~/code/la", "/Users/you/code", folder("lamphaus"))).toBe(
      "/Users/you/code/lamphaus",
    );
    expect(root("~/code/new-thing", "/Users/you/code", null)).toBe("/Users/you/code/new-thing");
    expect(root("~/code/", "/Users/you/code", null)).toBe("/Users/you/code");
    expect(root("/opt/repo/", "/opt/repo", null)).toBe("/opt/repo");
  });
});

describe("ARC-24 matching what is typed", () => {
  it("ranks letters at a word's start before letters inside one, and finds them there", () => {
    expect(rank("Restart the session", "res")).toBe(0);
    expect(rank("Flaky resize test", "res")).toBe(0);
    expect(rank("Put back a turn, restoring", "tor")).toBe(1);
    expect(rank("Rename", "zz")).toBeNull();
    expect(found("Split view.rs by responsibility", "res")).toBe(17);
    expect(found("caress the resize", "res")).toBe(11);
  });

  it("keeps each kind's order among equals", () => {
    expect(narrowed(["presets", "Reset", "Restart"], "res", (s) => s)).toEqual([
      "Reset",
      "Restart",
      "presets",
    ]);
  });
});
