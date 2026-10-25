// The projects moved by a drag up or down the list (PRD §6a), as the
// terminal UI's `app::project_order` moves them.

import { describe, expect, it } from "vitest";
import { movedProjects } from "./projectOrder";

describe("a project carried onto another", () => {
  it("goes after it moving down and before it moving up", () => {
    const order = ["a", "b", "c"];
    expect(movedProjects(order, "a", "c")).toEqual(["b", "c", "a"]);
    expect(movedProjects(order, "c", "a")).toEqual(["c", "a", "b"]);
    expect(movedProjects(order, "b", "c")).toEqual(["a", "c", "b"]);
    expect(movedProjects(order, "a", "a")).toBeNull();
    expect(movedProjects(order, "a", "z")).toBeNull();
  });
});
