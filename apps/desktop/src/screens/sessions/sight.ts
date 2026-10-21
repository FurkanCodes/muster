// Which lines of a long tree are in sight (ARC-25): each line's place is
// the heights of those above it, as measured once drawn or, before, as its
// kind is drawn; only the lines in sight, and a few beyond, are drawn,
// between two spacers as tall as the lines left out. A line's height
// includes the space above it (sessions.css), so the places add up.

import type { TreeRow } from "./tree";

/** Pixels drawn beyond the edges of the list, so a scroll shows no gap. */
export const OVERSCAN = 320;

/** The list's height until it is measured (and where nothing is laid out, as in tests). */
export const UNMEASURED = 800;

/** A line's height before it is drawn: its kind's (the design system's sizes), and the space above it. */
export function estimate(row: TreeRow, first: boolean, lines: "one" | "two"): number {
  const above = first ? 0 : row.kind === "project" ? 16 : 2;
  // A shell's line is shorter; measured once drawn.
  const own = row.kind === "session" ? (lines === "two" ? 46 : 32) : 28;
  return above + own;
}

/** Where each line begins, and after the last, where the list ends: one more than the lines. */
export function placesOf(heights: readonly number[]): number[] {
  const places = [0];
  let at = 0;
  for (const height of heights) {
    at += height;
    places.push(at);
  }
  return places;
}

/** The line at `y`: the last whose place is at or above it. */
export function lineAt(places: readonly number[], y: number): number {
  let low = 0;
  let high = places.length - 2;
  if (high < 0) return 0;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if ((places[middle] ?? 0) <= y) low = middle;
    else high = middle - 1;
  }
  return low;
}

/** The lines drawn, `[start, end)`, for a list scrolled to `top` and `height` tall. */
export function drawn(places: readonly number[], top: number, height: number): [number, number] {
  const count = places.length - 1;
  if (count <= 0) return [0, 0];
  const start = lineAt(places, Math.max(0, top - OVERSCAN));
  const end = Math.min(count, lineAt(places, top + height + OVERSCAN) + 1);
  return [start, end];
}

/** Where the list scrolls to from `top` so the line from `from` to `to` is in sight among `height` pixels. */
export function inSight(top: number, height: number, from: number, to: number): number {
  if (from < top) return from;
  return to > top + height ? to - height : top;
}
