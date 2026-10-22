import { describe, expect, it } from "vitest";
import type { TerminalModes } from "../ipc/generated/TerminalModes";
import { type Wheel, WheelLines, wheelReport } from "./wheel";

const PIXEL = 0;
const LINE = 1;
const PAGE = 2;
const grid = { cellHeight: 20, rows: 40 };

const wheel = (deltaY: number, deltaMode = PIXEL, keys: Partial<Wheel> = {}): Wheel => ({
  deltaY,
  deltaMode,
  altKey: false,
  ctrlKey: false,
  shiftKey: false,
  ...keys,
});

const modes = (over: Partial<TerminalModes> = {}): TerminalModes => ({
  kitty_flags: 0,
  bracketed_paste: true,
  application_cursor: false,
  mouse: "any_motion",
  mouse_encoding: "sgr",
  alternate_screen: true,
  ...over,
});

describe("ARC-24 the wheel, for a program that takes the mouse", () => {
  it("adds a trackpad's small steps up to whole lines, none lost", () => {
    const lines = new WheelLines();
    // Twenty-four 5 px steps go six lines of 20 px: one every fourth step.
    const taken = Array.from({ length: 24 }, () => lines.take(wheel(5), grid));
    expect(taken.filter((count) => count !== 0)).toEqual([1, 1, 1, 1, 1, 1]);
  });

  it("goes the whole way of a flick, not one line an event", () => {
    expect(new WheelLines().take(wheel(-130), grid)).toBe(-6);
  });

  it("turns at once when the wheel turns back", () => {
    const lines = new WheelLines();
    expect(lines.take(wheel(15), grid)).toBe(0);
    expect(lines.take(wheel(-25), grid)).toBe(-1);
  });

  it("counts a mouse's lines and pages as they are", () => {
    expect(new WheelLines().take(wheel(3, LINE), grid)).toBe(3);
    expect(new WheelLines().take(wheel(-1, PAGE), grid)).toBe(-40);
    expect(new WheelLines().take(wheel(1e9), grid)).toBe(100);
  });

  it("reports in SGR where the pointer is, with Alt and Ctrl", () => {
    expect(wheelReport(true, wheel(-1), 9, 4, modes())).toBe("\x1b[<64;10;5M");
    expect(wheelReport(false, wheel(1, PIXEL, { ctrlKey: true }), 0, 0, modes())).toBe(
      "\x1b[<81;1;1M",
    );
  });

  it("reports in the original encodings as far as they reach", () => {
    const old = modes({ mouse: "press_release", mouse_encoding: "default" });
    expect(wheelReport(true, wheel(-1), 9, 4, old)).toBe("\x1b[M`*%");
    expect(wheelReport(true, wheel(-1), 120, 4, old)).toBeNull();
    const utf8 = modes({ mouse_encoding: "utf8" });
    expect(wheelReport(false, wheel(1), 300, 4, utf8)).toBe(
      `\x1b[M${String.fromCodePoint(97, 333, 37)}`,
    );
  });

  it("reports nothing to a program that did not ask, and X10 mode no keys", () => {
    expect(wheelReport(true, wheel(-1), 0, 0, modes({ mouse: "off" }))).toBeNull();
    expect(
      wheelReport(true, wheel(-1, PIXEL, { altKey: true }), 0, 0, modes({ mouse: "press" })),
    ).toBe("\x1b[<64;1;1M");
  });
});
