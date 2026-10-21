// Moving the panels with the keys (`m`, `move_panels`; `app::drag` in the
// terminal UI): the panel with the keys is picked, ⇥ picks another, the
// arrows move it past its neighbour that way or to the pane's edge, `+` and
// `-` give it more or less of its place, ⏎ keeps the layout and Esc puts it
// back. While it lasts, those keys are its own.

import { useEffect, useState } from "react";
import { useActions, useDaemon } from "../app/context";
import { useKeys } from "../app/keys";
import { chordOfWritten } from "./beside";
import type { Edge, Layout, PanelName } from "./layout";
import type { Rect, Solved } from "./solve";
import type { PanelsLayout } from "./useLayout";
import { writtenKeys } from "./usePanelKeys";

const EDGES: Readonly<Record<string, Edge>> = {
  ArrowLeft: "left",
  ArrowRight: "right",
  ArrowUp: "top",
  ArrowDown: "bottom",
};

const NAMES: Readonly<Record<PanelName, string>> = {
  list: "the list",
  session: "the agent",
  changes: "the changes",
  shell: "the shell",
  plugin: "the plugin's panel",
};

/** The nearest panel past `area`'s `edge`, overlapping it the other way (`drag::neighbour`). */
export function neighbour(shown: Solved["panels"], area: Rect, edge: Edge): PanelName | null {
  let best: { gap: number; panel: PanelName } | null = null;
  for (const { panel, rect } of shown) {
    const across = rect.y < area.y + area.height && area.y < rect.y + rect.height;
    const down = rect.x < area.x + area.width && area.x < rect.x + rect.width;
    let gap: number | null = null;
    if (edge === "left" && across && rect.x + rect.width <= area.x)
      gap = area.x - (rect.x + rect.width);
    if (edge === "right" && across && rect.x >= area.x + area.width)
      gap = rect.x - (area.x + area.width);
    if (edge === "top" && down && rect.y + rect.height <= area.y)
      gap = area.y - (rect.y + rect.height);
    if (edge === "bottom" && down && rect.y >= area.y + area.height)
      gap = rect.y - (area.y + area.height);
    if (gap !== null && (!best || gap < best.gap)) best = { gap, panel };
  }
  return best?.panel ?? null;
}

/** `layout` with `panel` given `by` percent more (or less) of the innermost place it shares. */
function resized(layout: Layout, solved: Solved, panel: PanelName, by: number): Layout {
  const divider = solved.dividers.findLast(
    (one) => one.first.includes(panel) || one.second.includes(panel),
  );
  if (!divider) return layout;
  const path = layout.splitBetween(divider.first, divider.second);
  if (!path) return layout;
  const across = divider.split === "across";
  const total = Math.max(1, across ? divider.area.width : divider.area.height);
  const first = across ? divider.line.x - divider.area.x : divider.line.y - divider.area.y;
  const now = Math.round((first * 100) / total);
  const share = Math.min(90, Math.max(10, now + (divider.first.includes(panel) ? by : -by)));
  return layout.sized(path, "first", { share });
}

export function useArranging(
  panels: PanelsLayout,
  solved: Solved,
  withKeys: PanelName,
  movable: boolean,
  pane: readonly PanelName[],
): PanelName | null {
  const actions = useActions();
  const keymap = useDaemon((state) => state.preferences?.keys);
  const [arranging, setArranging] = useState<PanelName | null>(null);

  const start = () => {
    if (!movable) return actions.tell("Nothing stands beside the session to move.", true);
    setArranging(withKeys);
    actions.tell(
      `Moving ${NAMES[withKeys]}: the arrows move it, ⇥ picks another, + and − size it, ⏎ keeps it, ⎋ puts it back.`,
    );
  };
  const handlers: Record<string, () => void> = {};
  for (const key of writtenKeys(keymap, "move_panels", ["m"]))
    handlers[chordOfWritten(key)] = start;
  useKeys(handlers);

  useEffect(() => {
    if (!arranging) return;
    const onKey = (event: KeyboardEvent) => {
      const order = solved.panels.map((one) => one.panel);
      const area = solved.panels.find((one) => one.panel === arranging)?.rect;
      const edge = EDGES[event.key];
      let handled = true;
      if (event.key === "Tab") {
        const at = order.indexOf(arranging);
        const next = order[(at + (event.shiftKey ? order.length - 1 : 1)) % order.length];
        if (next) setArranging(next);
      } else if (edge && area) {
        const past = neighbour(solved.panels, area, edge);
        panels.show(
          past
            ? panels.layout.moved(arranging, { beside: past, edge })
            : panels.layout.movedToEdgeOf(arranging, pane, edge),
        );
      } else if (event.key === "+" || event.key === "=" || event.key === "-") {
        panels.show(resized(panels.layout, solved, arranging, event.key === "-" ? -5 : 5));
      } else if (event.key === "Enter") {
        setArranging(null);
        panels.keep(panels.layout);
        actions.clearNote();
      } else if (event.key === "Escape") {
        setArranging(null);
        panels.show(null);
        actions.clearNote();
      } else handled = false;
      if (!handled) return;
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [arranging, solved, panels, pane, actions]);

  // Nothing left to move beside it: the keys are the list's again.
  useEffect(() => {
    if (!movable && arranging) {
      setArranging(null);
      panels.show(null);
    }
  }, [movable, arranging, panels]);

  return arranging;
}
