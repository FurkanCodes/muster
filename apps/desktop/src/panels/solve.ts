// The panels shown laid out over the pane, as the terminal UI lays them out
// over its window (`muster-tui`'s `layout::solve`), in pixels: each split
// shares its place as its parts were sized (a share, or cells of the
// terminal's type), else half, each part keeping at least what a panel
// needs, the line between them `GAP` wide.

import type { Branch, Edge, LayoutNode, PanelName, Size, Split } from "./layout";
import { panelsIn } from "./layout";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The line between two parts, which a drag resizes. */
export interface Divider {
  path: Branch[];
  split: Split;
  /** The place the two share. */
  area: Rect;
  first: PanelName[];
  second: PanelName[];
  /** The line itself. */
  line: Rect;
}

export interface Solved {
  panels: { panel: PanelName; rect: Rect }[];
  dividers: Divider[];
}

/** The line between two panels, which is also the gap the design system keeps (`space-8`). */
export const GAP = 8;
/** A terminal cell in the panels' type (JetBrains Mono 13 on 20), for sizes given in cells. */
export const CELL = { width: 8, height: 20 } as const;
/** The least a panel is given (`layout-panel-min` wide; its head and a few lines high). */
export const PANEL_LEAST = { width: 320, height: 140 } as const;

/** The least `node` needs, across and down. */
export function needs(node: LayoutNode): { width: number; height: number } {
  const part = node.part;
  if (part.kind === "panel") return { ...PANEL_LEAST };
  const first = needs(part.first);
  const second = needs(part.second);
  return part.split === "across"
    ? { width: first.width + GAP + second.width, height: Math.max(first.height, second.height) }
    : { width: Math.max(first.width, second.width), height: first.height + GAP + second.height };
}

function pixels(size: Size, total: number, split: Split): number {
  if ("share" in size) return Math.floor((total * size.share) / 100);
  return Math.min(total, size.cells * (split === "across" ? CELL.width : CELL.height));
}

/** What the first of two sharing `total` wants: as either was sized, or half. */
function wanted(first: LayoutNode, second: LayoutNode, total: number, split: Split): number {
  if (first.size) return pixels(first.size, total, split);
  if (second.size) return total - pixels(second.size, total, split);
  return Math.floor(total / 2);
}

function place(node: LayoutNode, area: Rect, path: Branch[], solved: Solved): void {
  const part = node.part;
  if (part.kind === "panel") {
    solved.panels.push({ panel: part.panel, rect: area });
    return;
  }
  const across = part.split === "across";
  const total = Math.max(0, (across ? area.width : area.height) - GAP);
  const along = (need: { width: number; height: number }) => (across ? need.width : need.height);
  const [firstLeast, secondLeast] = [along(needs(part.first)), along(needs(part.second))];
  const want = wanted(part.first, part.second, total, part.split);
  const cut =
    total >= firstLeast + secondLeast
      ? Math.min(Math.max(want, firstLeast), total - secondLeast)
      : Math.min(want, total);
  const firstArea = across ? { ...area, width: cut } : { ...area, height: cut };
  const line = across
    ? { ...area, x: area.x + cut, width: GAP }
    : { ...area, y: area.y + cut, height: GAP };
  const secondArea = across
    ? { ...area, x: area.x + cut + GAP, width: total - cut }
    : { ...area, y: area.y + cut + GAP, height: total - cut };
  solved.dividers.push({
    path: [...path],
    split: part.split,
    area,
    first: panelsIn(part.first),
    second: panelsIn(part.second),
    line,
  });
  place(part.first, firstArea, [...path, "first"], solved);
  place(part.second, secondArea, [...path, "second"], solved);
}

export function solve(node: LayoutNode, area: Rect): Solved {
  const solved: Solved = { panels: [], dividers: [] };
  place(node, area, [], solved);
  return solved;
}

/** The least share either of two can be dragged to (`app::drag`). */
const LEAST_SHARE = 10;

/** The share the first of a divider's two takes once its line is at `at` (pixels along its place). */
export function shareAt(divider: Divider, at: number): number {
  const across = divider.split === "across";
  const total = Math.max(1, (across ? divider.area.width : divider.area.height) - GAP);
  const first = at - (across ? divider.area.x : divider.area.y);
  const share = Math.round((first * 100) / total);
  return Math.min(100 - LEAST_SHARE, Math.max(LEAST_SHARE, share));
}

/** Where a panel dropped at a point would land, as it lights up. */
export type Landing = { edge: Edge; target: PanelName | null; rect: Rect };

/** How near the pane's edge a drop goes to that edge rather than beside a panel. */
export const EDGE_ZONE = 24;

/**
 * Where `panel` dropped at (`x`, `y`) in `pane` would land: along an edge of
 * the pane near it, or at the nearest edge of the panel under it. Over
 * itself, nowhere (`app::drag::drop_place`).
 */
export function landingAt(
  panel: PanelName,
  x: number,
  y: number,
  pane: Rect,
  shown: Solved["panels"],
): Landing | null {
  const edgeOfPane: Edge | null =
    x <= pane.x + EDGE_ZONE
      ? "left"
      : x >= pane.x + pane.width - EDGE_ZONE
        ? "right"
        : y >= pane.y + pane.height - EDGE_ZONE
          ? "bottom"
          : y <= pane.y + EDGE_ZONE
            ? "top"
            : null;
  if (edgeOfPane) return { edge: edgeOfPane, target: null, rect: half(pane, edgeOfPane, 3) };
  const under = shown.find(
    ({ rect }) => x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height,
  );
  if (!under || under.panel === panel) return null;
  const { rect } = under;
  const distances: [number, Edge][] = [
    [x - rect.x, "left"],
    [rect.x + rect.width - x, "right"],
    [y - rect.y, "top"],
    [rect.y + rect.height - y, "bottom"],
  ];
  const [, edge] = distances.reduce((near, one) => (one[0] < near[0] ? one : near));
  return { edge, target: under.panel, rect: half(rect, edge, 2) };
}

/** The `1 / parts` of `rect` along its `edge`. */
function half(rect: Rect, edge: Edge, parts: number): Rect {
  const width = Math.round(rect.width / parts);
  const height = Math.round(rect.height / parts);
  switch (edge) {
    case "left":
      return { ...rect, width };
    case "right":
      return { ...rect, x: rect.x + rect.width - width, width };
    case "top":
      return { ...rect, height };
    case "bottom":
      return { ...rect, y: rect.y + rect.height - height, height };
  }
}
