import { describe, expect, it } from "vitest";
import { doing } from "./status";

describe("what a session is doing", () => {
  it("says what its agent does, else how it stands", () => {
    const at = { text: "editing view.rs", since: 0 };
    expect(doing({ status: { state: "running" }, activity: at })).toBe("editing view.rs");
    expect(doing({ status: { state: "finished" }, activity: null })).toBe("finished");
  });

  it("says an agent started it where one did (PRD-01)", () => {
    const session = { status: { state: "finished" as const }, activity: null, started_by: "s1" };
    expect(doing(session)).toBe("finished, started by an agent");
  });
});
