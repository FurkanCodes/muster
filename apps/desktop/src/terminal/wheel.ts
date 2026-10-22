// The wheel, for a program that takes the mouse (Claude Code does, on its
// full-screen views): it is sent a report for every line the wheel goes,
// as the person's own terminal sends, and as the terminal UI does
// (`muster-tui`'s `mouse.rs`). xterm.js sends such a program at most one
// report a wheel event, and scales a trackpad's small steps by 0.3, so a
// trackpad hardly moves it and a flick loses most of its way.

import type { TerminalModes } from "../ipc/generated/TerminalModes";

/** A wheel event, as much of it as the reports need. */
export interface Wheel {
  deltaY: number;
  /** `WheelEvent.DOM_DELTA_PIXEL`, `_LINE` or `_PAGE`. */
  deltaMode: number;
  altKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
}

/** The terminal's grid, in CSS pixels, as drawn. */
export interface Grid {
  cellHeight: number;
  rows: number;
}

/** The most reports one wheel event sends: a page's worth. */
const MOST_AT_ONCE = 100;

const DOM_DELTA_LINE = 1;
const DOM_DELTA_PAGE = 2;

/**
 * Lines the wheel went, kept between events: a trackpad goes a fraction of
 * a line an event, and the fractions add up to lines. Turning back starts
 * afresh, so the program turns at once.
 */
export class WheelLines {
  private partial = 0;

  /** The whole lines this event completes: negative up, positive down. */
  take(wheel: Wheel, grid: Grid): number {
    const lines =
      wheel.deltaMode === DOM_DELTA_PAGE
        ? wheel.deltaY * grid.rows
        : wheel.deltaMode === DOM_DELTA_LINE
          ? wheel.deltaY
          : grid.cellHeight > 0
            ? wheel.deltaY / grid.cellHeight
            : 0;
    if (lines === 0 || !Number.isFinite(lines)) return 0;
    if (Math.sign(lines) !== Math.sign(this.partial)) this.partial = 0;
    this.partial += lines;
    const whole = Math.trunc(this.partial);
    this.partial -= whole;
    return Math.max(-MOST_AT_ONCE, Math.min(MOST_AT_ONCE, whole));
  }
}

/**
 * One wheel report at `column`, `row` (counted from 0 on the program's
 * screen), up or down, encoded as the program asked; `null` if it asked
 * for no mouse, or its encoding cannot carry the place from here.
 */
export function wheelReport(
  up: boolean,
  wheel: Wheel,
  column: number,
  row: number,
  modes: TerminalModes,
): string | null {
  if (modes.mouse === "off") return null;
  let code = up ? 64 : 65;
  // X10 mode reports buttons alone; Shift is the terminal's own, as xterm's.
  if (modes.mouse !== "press") {
    if (wheel.altKey) code += 8;
    if (wheel.ctrlKey) code += 16;
  }
  const x = column + 1;
  const y = row + 1;
  if (modes.mouse_encoding === "sgr") return `\x1b[<${code};${x};${y}M`;
  const values = [code + 32, x + 32, y + 32];
  // Input goes to the daemon as text, so the original encoding's bytes past
  // 127 would arrive as two: it reaches as far as 95 columns from here.
  const most = modes.mouse_encoding === "utf8" ? 2047 : 127;
  if (values.some((value) => value > most)) return null;
  return `\x1b[M${String.fromCodePoint(...values)}`;
}
