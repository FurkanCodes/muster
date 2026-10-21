import { describe, expect, it } from "vitest";
import { drawn, estimate, inSight, lineAt, OVERSCAN, placesOf } from "./sight";

describe("ARC-25 the lines of a long tree in sight", () => {
  it("places each line under the heights of those above it", () => {
    expect(placesOf([28, 30, 46])).toEqual([0, 28, 58, 104]);
    expect(placesOf([])).toEqual([0]);
  });

  it("finds the line at a height, the last that begins at or above it", () => {
    const places = placesOf([10, 10, 10, 10]);
    expect(lineAt(places, 0)).toBe(0);
    expect(lineAt(places, 9)).toBe(0);
    expect(lineAt(places, 10)).toBe(1);
    expect(lineAt(places, 35)).toBe(3);
    expect(lineAt(places, 1e6)).toBe(3);
    expect(lineAt(placesOf([]), 5)).toBe(0);
  });

  it("draws the lines in sight and a few hundred pixels beyond, never all of thousands", () => {
    const places = placesOf(Array.from({ length: 5000 }, () => 50));
    expect(drawn(places, 0, 800)).toEqual([0, Math.floor((800 + OVERSCAN) / 50) + 1]);
    const [start, end] = drawn(places, 100_000, 800);
    expect(start).toBe(Math.floor((100_000 - OVERSCAN) / 50));
    expect(end - start).toBeLessThan(60);
    expect(drawn(places, 250_000, 800)[1]).toBe(5000);
    expect(drawn(placesOf([]), 0, 800)).toEqual([0, 0]);
  });

  it("scrolls up to a line above, down just far enough to one below, and not at all to one in sight", () => {
    expect(inSight(500, 400, 100, 150)).toBe(100);
    expect(inSight(500, 400, 950, 1000)).toBe(600);
    expect(inSight(500, 400, 600, 650)).toBe(500);
  });

  it("estimates a line by its kind, with the space above it", () => {
    const row = (kind: "project" | "session") => ({
      kind,
      id: "a",
      project: "p",
      workspace: null,
    });
    expect(estimate(row("project"), true, "two")).toBe(28);
    expect(estimate(row("project"), false, "two")).toBe(44);
    expect(estimate(row("session"), false, "two")).toBe(48);
    expect(estimate(row("session"), false, "one")).toBe(34);
  });
});
