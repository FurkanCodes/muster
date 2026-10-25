// The panels in the session's pane (design system: Panel; `PRODUCT.md` §6,
// `ARCHITECTURE.md` §5a Panels): the preview (the session the list selects),
// a plugin's panel and every terminal placed in a panel of its own, an
// agent's or a shell's, each where the person's layout puts it. A session's
// line carried from the sidebar by the pointer and let go on the pane is
// placed where it lands (`carry`); `P` places the session shown, and again takes it off the
// screen. A panel dragged by its head goes to an edge of the pane, or beside
// or under another, the place lighting up as it would land, or, dropped
// about another's middle, swaps places with it; the line between two is
// dragged to share their place; either is kept in the person's settings. A
// double-click on a head zooms its panel over the others, and again puts it
// back. Each panel is placed by its own rectangle in one box, so moving one
// never starts its terminal afresh. Where they do not fit, the panel with the
// keys opens over the rest, saying how many it hides. Alone, the agent's
// terminal is drawn as it is. `m` moves them with the keys instead
// (useArranging), and the panel key's keys act on them (usePrefixKeys).

import { type PointerEvent, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { panelBeside, running } from "./beside";
import { type Carrying, onCarrying, useTakesCarried } from "./carry";
import { type Layout, type PanelName, placedPanel } from "./layout";
import { PanelLine, PlacedPanel, PluginPanel, SessionPanel, TerminalAlone } from "./PanelBoxes";
import {
  type Divider,
  GAP,
  type Landing,
  landingAt,
  needs,
  type Rect,
  type Solved,
  shareAt,
  solve,
} from "./solve";
import { useArranging } from "./useArranging";
import { useLayout } from "./useLayout";
import { usePanelKeys } from "./usePanelKeys";
import { usePrefixKeys } from "./usePrefixKeys";
import "./panels.css";

/** The panels of the session's pane, as `movedToEdgeOf` reads the pane's edges. */
const PANE: readonly PanelName[] = ["session", "changes", "plugin"];

/** How far a pressed head moves before it is a drag rather than a click. */
const DRAG_FROM = 4;

/** The box's size, measured as it changes (one observer, no polling). */
function useBoxSize(box: React.RefObject<HTMLDivElement | null>) {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const element = box.current;
    // Unmeasured (jsdom has no observer), every panel is laid out at no size.
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      const { width, height } = entry.contentRect;
      setSize((was) =>
        was.width === Math.round(width) && was.height === Math.round(height)
          ? was
          : { width: Math.round(width), height: Math.round(height) },
      );
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [box]);
  return size;
}

/**
 * The panels laid out over `area`: all that are shown, or the zoomed one
 * alone, or the one with the keys over the rest where they do not fit; and
 * how many of those shown it hides then.
 */
function laidOut(
  layout: Layout,
  shown: (panel: PanelName) => boolean,
  withKeys: PanelName,
  zoomed: PanelName | null,
  area: Rect,
): { solved: Solved; hiding: number } {
  const count = layout.shown(shown);
  const total = count ? panelsCount(count) : 0;
  const all = zoomed ? layout.shown((panel) => panel === zoomed) : count;
  if (all) {
    const least = needs(all);
    if (area.width === 0 || (least.width <= area.width && least.height <= area.height))
      return { solved: solve(all, area), hiding: zoomed ? total - 1 : 0 };
  }
  return {
    solved: { panels: [{ panel: zoomed ?? withKeys, rect: area }], dividers: [] },
    hiding: total - 1,
  };
}

/** How many panels `node` holds. */
function panelsCount(node: NonNullable<ReturnType<Layout["shown"]>>): number {
  return node.part.kind === "panel"
    ? 1
    : panelsCount(node.part.first) + panelsCount(node.part.second);
}

function lineKey(divider: Divider): string {
  return `${divider.first.join()}|${divider.second.join()}`;
}

/** The share the first of a line's two has now, as the person reads it on the line. */
function shareOf(divider: Divider): number {
  const across = divider.split === "across";
  const first = across ? divider.line.x - divider.area.x : divider.line.y - divider.area.y;
  const total = Math.max(1, (across ? divider.area.width : divider.area.height) - GAP);
  return Math.round((first * 100) / total);
}

type Drag =
  | { kind: "panel"; panel: PanelName; x: number; y: number; moved: boolean; to: Landing | null }
  | { kind: "line"; divider: Divider; was: Layout };

export function Panels({ session }: { session: SessionId }) {
  const app = useApp();
  const box = useRef<HTMLDivElement>(null);
  const size = useBoxSize(box);
  const panels = useLayout();
  const plugin = useDaemon((state) => panelBeside(state, session));
  const typingIn = useUi((state) => (state.keysIn === "terminal" ? state.terminal : null));
  // The sessions placed that the daemon still has, in the layout's order;
  // a shell that ended leaves its panel as the daemon takes it out.
  const placed = useDaemon(
    useShallow((state) =>
      panels.layout.placed().filter((id) => {
        const stored = state.sessions[id];
        return stored && (stored.harness !== "shell" || running(stored));
      }),
    ),
  );
  const [drag, setDrag] = useState<Drag | null>(null);
  const [dropping, setDropping] = useState<Landing | null>(null);
  usePanelKeys(session, plugin, panels);

  // The session shown, where it is placed, is seen in its own panel; the
  // preview keeps the last one shown that is not, or goes, so every terminal
  // on the pane is drawn and none twice. Two are kept, the latest first, as a
  // line carried from the sidebar may be shown as it is taken up.
  const shownPlaced = placed.includes(session);
  const [kept, setKept] = useState<SessionId[]>([session]);
  if (!shownPlaced && kept[0] !== session) setKept([session, ...kept.slice(0, 1)]);
  const keptAgent = useDaemon(
    (state) => kept.find((id) => id in state.sessions && !placed.includes(id)) ?? null,
  );
  // The preview folded away by its ✕ comes back as another is selected.
  const [folded, setFolded] = useState<SessionId | null>(null);
  if (folded && folded !== session && folded !== keptAgent) setFolded(null);
  const shownAgent = shownPlaced ? keptAgent : session;
  // Alone, the session is drawn as it is, folded or not.
  const alone = !plugin && placed.length === 0;
  const agent =
    !alone && shownAgent && shownAgent === folded && typingIn !== folded ? null : shownAgent;

  const sessionOf: Partial<Record<PanelName, SessionId>> = {};
  if (agent) sessionOf.session = agent;
  if (plugin) sessionOf.plugin = plugin;
  for (const id of placed) sessionOf[placedPanel(id)] = id;
  const withKeys: PanelName =
    typingIn && typingIn === plugin
      ? "plugin"
      : typingIn && placed.includes(typingIn)
        ? placedPanel(typingIn)
        : shownPlaced
          ? placedPanel(session)
          : "session";
  // Zoomed, one panel shows alone, following the keys from panel to panel.
  const [zoomedAs, setZoomed] = useState<PanelName | null>(null);
  const zoomed = zoomedAs && sessionOf[zoomedAs] ? zoomedAs : null;
  if (zoomed && withKeys !== zoomed && typingIn && sessionOf[withKeys]) setZoomed(withKeys);
  const area = { x: 0, y: 0, width: size.width, height: size.height };
  const { solved, hiding } = laidOut(
    panels.layout,
    (panel) => Boolean(sessionOf[panel]),
    withKeys,
    zoomed,
    area,
  );
  const pane: PanelName[] = [...PANE, ...placed.map(placedPanel)];
  const arranging = useArranging(panels, solved, withKeys, !alone, pane);
  const zoom = (panel: PanelName) => () => setZoomed(zoomed ? null : panel);
  usePrefixKeys({
    panels,
    solved,
    sessionOf,
    withKeys,
    shown: session,
    zoom: (panel) => setZoomed(zoomed ? null : panel),
    fold: (id) => setFolded(id),
  });
  const hidingOf = (panel: PanelName) =>
    hiding > 0 && solved.panels.some((one) => one.panel === panel) ? hiding : null;

  // A drag lets go on Esc, the layout as it was.
  useEffect(() => {
    if (!drag) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      panels.show(null);
      setDrag(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [drag, panels]);

  const point = (event: { clientX: number; clientY: number }) => {
    const bounds = box.current?.getBoundingClientRect();
    return { x: event.clientX - (bounds?.left ?? 0), y: event.clientY - (bounds?.top ?? 0) };
  };

  const headDown = (panel: PanelName) => (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || alone) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const at = point(event);
    setDrag({ kind: "panel", panel, x: at.x, y: at.y, moved: false, to: null });
  };
  const lineDown = (divider: Divider) => (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    setDrag({ kind: "line", divider, was: panels.layout });
  };
  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!drag) return;
    const at = point(event);
    if (drag.kind === "panel") {
      const moved = drag.moved || Math.hypot(at.x - drag.x, at.y - drag.y) >= DRAG_FROM;
      const to = moved ? landingAt(drag.panel, at.x, at.y, area, solved.panels, true) : null;
      if (moved !== drag.moved || to?.rect !== drag.to?.rect) setDrag({ ...drag, moved, to });
      return;
    }
    const { divider, was } = drag;
    const path = was.splitBetween(divider.first, divider.second);
    if (!path) return;
    const share = shareAt(divider, divider.split === "across" ? at.x : at.y);
    panels.show(was.sized(path, "first", { share }));
  };
  const onUp = () => {
    if (!drag) return;
    setDrag(null);
    if (drag.kind === "line") return panels.keep(panels.layout);
    if (!drag.moved) {
      // Pressed and not moved: the panel gets the keys, as a click on it does.
      const target = sessionOf[drag.panel];
      if (target) {
        app.actions.showTerminal(target);
        app.actions.keysTo("terminal");
      }
      return;
    }
    const to = drag.to;
    if (!to) return;
    panels.keep(
      to.target && to.over
        ? panels.layout.moved(drag.panel, { over: to.target })
        : to.target
          ? panels.layout.moved(drag.panel, { beside: to.target, edge: to.edge })
          : panels.layout.movedToEdgeOf(drag.panel, pane, to.edge),
    );
  };

  // A session's line carried over the pane: where it would be placed lights up.
  const landingOf = (carrying: Carrying | null) => {
    const bounds = box.current?.getBoundingClientRect();
    if (carrying?.what.kind !== "session" || !bounds) return null;
    const { x, y } = carrying;
    if (x < bounds.left || x > bounds.right || y < bounds.top || y > bounds.bottom) return null;
    const id = carrying.what.id;
    const to = landingAt(placedPanel(id), x - bounds.left, y - bounds.top, area, solved.panels);
    return to ? { id, to } : null;
  };
  useEffect(() =>
    onCarrying((carrying) => {
      const to = landingOf(carrying)?.to ?? null;
      setDropping((was) => {
        if (was && to && was.rect === to.rect) return was;
        return to;
      });
    }),
  );
  // Let go here, it is placed where it lands: beside a panel, or at the pane's edge.
  useTakesCarried((carrying) => {
    const landed = landingOf(carrying);
    if (!landed) return false;
    const { id, to } = landed;
    const panel = placedPanel(id);
    panels.keep(
      to.target
        ? panels.layout.placing(id, { beside: to.target, edge: to.edge })
        : panels.layout
            .placing(id, { beside: "session", edge: to.edge })
            .movedToEdgeOf(panel, [...pane, panel], to.edge),
    );
    app.actions.tell("Placed it in a panel of its own.");
    return true;
  });

  const placedAt = (panel: PanelName) => solved.panels.find((one) => one.panel === panel)?.rect;
  const style = (rect: Rect | undefined) =>
    rect
      ? { left: rect.x, top: rect.y, width: rect.width, height: rect.height }
      : { display: "none" as const };
  const dragging = drag?.kind === "panel" && drag.moved ? drag.panel : arranging;
  const lineDragged = drag?.kind === "line" ? lineKey(drag.divider) : null;

  return (
    <div
      ref={box}
      className={`panels ${drag ? "panels--dragging" : ""}`}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={() => {
        panels.show(null);
        setDrag(null);
      }}
    >
      {alone ? (
        <TerminalAlone session={session} />
      ) : (
        <>
          {agent ? (
            <SessionPanel
              session={agent}
              style={style(placedAt("session"))}
              lifted={dragging === "session"}
              onHeadDown={headDown("session")}
              onZoom={zoom("session")}
              hiding={hidingOf("session")}
              among={
                placed.length > 0 || plugin
                  ? {
                      onPlace: () =>
                        panels.keep(
                          panels.layout.placing(agent, { beside: "session", edge: "left" }),
                        ),
                      onFold: () => setFolded(agent),
                    }
                  : null
              }
            />
          ) : null}
          {plugin ? (
            <PluginPanel
              panel={plugin}
              shown={session}
              style={style(placedAt("plugin"))}
              lifted={dragging === "plugin"}
              onHeadDown={headDown("plugin")}
              onZoom={zoom("plugin")}
              hiding={hidingOf("plugin")}
            />
          ) : null}
          {placed.map((id) => (
            <PlacedPanel
              key={id}
              session={id}
              style={style(placedAt(placedPanel(id)))}
              lifted={dragging === placedPanel(id)}
              onHeadDown={headDown(placedPanel(id))}
              onZoom={zoom(placedPanel(id))}
              hiding={hidingOf(placedPanel(id))}
              onTakeOff={() => panels.keep(panels.layout.unplaced(id))}
            />
          ))}
          {solved.dividers.map((divider) => {
            const key = lineKey(divider);
            return (
              <PanelLine
                key={key}
                divider={divider}
                share={shareOf(divider)}
                dragged={lineDragged === key}
                style={style(divider.line)}
                onPointerDown={lineDown(divider)}
                onShare={(share) => {
                  const path = panels.layout.splitBetween(divider.first, divider.second);
                  if (path) panels.keep(panels.layout.sized(path, "first", { share }));
                }}
              />
            );
          })}
          {drag?.kind === "panel" && drag.to ? (
            <div className="panels-landing" style={style(drag.to.rect)} aria-hidden="true" />
          ) : null}
        </>
      )}
      {dropping ? (
        <div className="panels-landing" style={style(dropping.rect)} aria-hidden="true" />
      ) : null}
    </div>
  );
}
