// What the key after the panel key does to the panels shown (`PRODUCT.md`
// §6a, the terminal UI's `app::prefix`; the panel key itself is the
// window's, `keys/panelKey`): on the panel with the keys, the preview's
// while the list has them, split it with a new shell in its workspace,
// move the keys to the panel that way (or the list past the left), swap
// with it, zoom it, take it off the screen or close its session as `x`
// does, place the preview's session, go to a panel by its number, give the
// keys back to the list, show these keys, or send the panel key itself to
// the program.

import { useEffect, useLayoutEffect, useRef } from "react";
import { type App, useApp } from "../app/context";
import { pressKey } from "../app/keys";
import type { SessionId } from "../ipc/generated/SessionId";
import { domChord } from "../keys/chord";
import { chordsOf } from "../keys/keymap";
import { answerPanelKeys } from "../keys/panelKey";
import { keymapFor } from "../keys/useKeymap";
import { giveKeys, startShell } from "./beside";
import { type Edge, type PanelName, placedSession } from "./layout";
import type { Rect, Solved } from "./solve";
import type { PanelsLayout } from "./useLayout";

export interface PrefixPanels {
  panels: PanelsLayout;
  solved: Solved;
  /** The session each panel shown holds. */
  sessionOf: Partial<Record<PanelName, SessionId>>;
  /** The panel with the keys, where a terminal has them. */
  withKeys: PanelName;
  /** The session the list selects. */
  shown: SessionId;
  zoom: (panel: PanelName) => void;
  fold: (session: SessionId) => void;
}

const EDGES: Readonly<Record<string, Edge>> = {
  focus_left: "left",
  focus_right: "right",
  focus_up: "top",
  focus_down: "bottom",
  swap_left: "left",
  swap_right: "right",
  swap_up: "top",
  swap_down: "bottom",
};

/** The nearest panel shown past `area`'s `edge`, overlapping it the other way (`drag::neighbour`). */
function neighbour(shown: Solved["panels"], area: Rect, edge: Edge): PanelName | null {
  let best: { gap: number; panel: PanelName } | null = null;
  for (const { panel, rect } of shown) {
    const across = rect.y < area.y + area.height && area.y < rect.y + rect.height;
    const down = rect.x < area.x + area.width && area.x < rect.x + rect.width;
    let gap: number | null = null;
    if (edge === "left" && across && rect.x + rect.width <= area.x)
      gap = area.x - rect.x - rect.width;
    if (edge === "right" && across && rect.x >= area.x + area.width)
      gap = rect.x - area.x - area.width;
    if (edge === "top" && down && rect.y + rect.height <= area.y)
      gap = area.y - rect.y - rect.height;
    if (edge === "bottom" && down && rect.y >= area.y + area.height)
      gap = rect.y - area.y - area.height;
    if (gap !== null && (!best || gap < best.gap)) best = { gap, panel };
  }
  return best?.panel ?? null;
}

/** The text the panel key, as the person set it, types: Ctrl and a letter is its control byte. */
function typed(app: App): string | null {
  const state = app.daemon.store.getState();
  const [chord] = chordsOf(keymapFor(state.preferences?.keys ?? {}, state.plugins), "panel_key");
  if (!chord || !("char" in chord.key)) return null;
  const char = chord.key.char;
  return chord.ctrl ? String.fromCharCode(char.toUpperCase().charCodeAt(0) & 0x1f) : char;
}

export function usePrefixKeys(shown: PrefixPanels): void {
  const app = useApp();
  const act = (action: string, chord: string) => {
    const { panels, solved, sessionOf, withKeys } = shown;
    const ui = app.ui.getState();
    const target: PanelName = ui.keysIn === "terminal" ? withKeys : "session";
    const area = solved.panels.find((one) => one.panel === target)?.rect;
    const edge = EDGES[action];
    if (action.startsWith("focus_") && edge) {
      const next = area ? neighbour(solved.panels, area, edge) : null;
      const session = next ? sessionOf[next] : undefined;
      if (session) giveKeys(app, session);
      else if (edge === "left") app.actions.keysTo("list");
      return;
    }
    if (action.startsWith("swap_") && edge) {
      const next = area ? neighbour(solved.panels, area, edge) : null;
      if (next) panels.keep(panels.layout.swapped(target, next));
      return;
    }
    switch (action) {
      case "split_right":
      case "split_down": {
        const session = sessionOf[target] ?? shown.shown;
        const workspace = app.daemon.store.getState().sessions[session]?.workspace;
        if (!workspace) return;
        const edge = action === "split_right" ? "right" : "bottom";
        void startShell(app, workspace, { beside: target, edge });
        return;
      }
      case "zoom":
        return shown.zoom(target);
      case "take_off": {
        const placed = placedSession(target);
        if (placed) panels.keep(panels.layout.unplaced(placed));
        else if (target === "session" && sessionOf.session) shown.fold(sessionOf.session);
        return;
      }
      case "close_panel": {
        const session = sessionOf[target];
        if (!session)
          return app.actions.tell("That panel has no session of its own to close.", true);
        // As `x` on its line does, asking while it runs.
        app.actions.select(ui.screen, session);
        app.actions.keysTo("list");
        const state = app.daemon.store.getState();
        const [close] = chordsOf(keymapFor(state.preferences?.keys ?? {}, state.plugins), "close");
        if (close) setTimeout(() => pressKey(domChord(close)), 0);
        return;
      }
      case "place_preview": {
        const preview = sessionOf.session;
        if (!preview) return app.actions.tell("The preview shows no session to place.", true);
        panels.keep(panels.layout.placing(preview, { beside: "session", edge: "left" }));
        return;
      }
      case "resize":
        return app.actions.tell(
          "Drag the line between two panels, or press Tab to it and use the arrows.",
        );
      case "to_panel": {
        const numbered = solved.panels
          .filter(({ panel }) => sessionOf[panel])
          .sort((one, other) => one.rect.y - other.rect.y || one.rect.x - other.rect.x);
        const at = Number(chord) - 1;
        const session = numbered[at] ? sessionOf[numbered[at].panel] : undefined;
        if (session) giveKeys(app, session);
        return;
      }
      case "to_list":
        return app.actions.keysTo("list");
      case "panel_keys":
        return app.actions.openPalette(true, "keys");
      case "send_panel_key": {
        const text = typed(app);
        if (text && ui.keysIn === "terminal" && ui.terminal)
          void app.bridge.input(ui.terminal, text).catch(() => {});
        return;
      }
      default:
        return;
    }
  };
  const latest = useRef(act);
  useLayoutEffect(() => {
    latest.current = act;
  });
  useEffect(() => {
    answerPanelKeys((action, chord) => latest.current(action, chord));
    return () => answerPanelKeys(null);
  }, []);
}
