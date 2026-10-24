// The panels in the session's pane (design system: Panel; `PRODUCT.md` §6,
// `ARCHITECTURE.md` §5a Panels): the session's agent, the shell beside it,
// a plugin's panel and the sessions pinned in panels of their own, each where
// the person's layout puts it. A session's line carried from the sidebar and
// dropped on the pane is pinned where it lands (`carry`); `P` pins the
// session shown, and again unpins it. A panel
// dragged by its head goes to an edge of the pane, or beside or under
// another, the place lighting up as it would land; the line between two
// is dragged to share their place; either is kept in the person's settings.
// Each panel is placed by its own rectangle in one box, so moving one never
// starts its terminal afresh. Where they do not fit, the panel with the
// keys opens over the rest. Alone, the agent's terminal is drawn as it is.
// `m` moves them with the keys instead (useArranging).

import { type DragEvent, type PointerEvent, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon, useUi } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { panelBeside, shellBeside } from "./beside";
import { CARRIED, carriedSession, carry } from "./carry";
import { type Layout, type PanelName, pinnedPanel } from "./layout";
import {
  PanelLine,
  PinnedPanel,
  PluginPanel,
  SessionPanel,
  ShellPanel,
  TerminalAlone,
} from "./PanelBoxes";
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
import "./panels.css";

/** The panels of the session's pane, as `movedToEdgeOf` reads the pane's edges. */
const PANE: readonly PanelName[] = ["session", "changes", "shell", "plugin"];

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

/** The panels laid out over `area`: all that are shown, or the one with the keys over the rest. */
function laidOut(
  layout: Layout,
  shown: (panel: PanelName) => boolean,
  withKeys: PanelName,
  area: Rect,
): Solved {
  const all = layout.shown(shown);
  if (all) {
    const least = needs(all);
    if (area.width === 0 || (least.width <= area.width && least.height <= area.height))
      return solve(all, area);
  }
  return { panels: [{ panel: withKeys, rect: area }], dividers: [] };
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
  const keptShell = useUi((state) => state.shell);
  const shell = useDaemon((state) => shellBeside(state, keptShell, session));
  const plugin = useDaemon((state) => panelBeside(state, session));
  const typingIn = useUi((state) => (state.keysIn === "terminal" ? state.terminal : null));
  // The sessions pinned that the daemon still has, in the layout's order.
  const pinned = useDaemon(
    useShallow((state) => panels.layout.pinned().filter((id) => id in state.sessions)),
  );
  const [drag, setDrag] = useState<Drag | null>(null);
  const [dropping, setDropping] = useState<Landing | null>(null);
  usePanelKeys(session, plugin, panels);

  // The session shown, where it is pinned, is seen in its own panel; the
  // agent's panel keeps the last one shown that is not, or goes, so every
  // terminal on the pane is drawn and none twice. Two are kept, the latest
  // first, as a line carried from the sidebar may be shown as it is taken up.
  const shownPinned = pinned.includes(session);
  const [kept, setKept] = useState<SessionId[]>([session]);
  if (!shownPinned && kept[0] !== session) setKept([session, ...kept.slice(0, 1)]);
  const keptAgent = useDaemon(
    (state) => kept.find((id) => id in state.sessions && !pinned.includes(id)) ?? null,
  );
  const agent = shownPinned ? keptAgent : session;

  const sessionOf: Partial<Record<PanelName, SessionId>> = {};
  if (agent) sessionOf.session = agent;
  if (shell) sessionOf.shell = shell;
  if (plugin) sessionOf.plugin = plugin;
  for (const id of pinned) sessionOf[pinnedPanel(id)] = id;
  const withKeys: PanelName =
    typingIn && typingIn === shell
      ? "shell"
      : typingIn && typingIn === plugin
        ? "plugin"
        : typingIn && pinned.includes(typingIn)
          ? pinnedPanel(typingIn)
          : shownPinned
            ? pinnedPanel(session)
            : "session";
  const area = { x: 0, y: 0, width: size.width, height: size.height };
  const solved = laidOut(panels.layout, (panel) => Boolean(sessionOf[panel]), withKeys, area);
  const alone = !shell && !plugin && pinned.length === 0;
  const pane: PanelName[] = [...PANE, ...pinned.map(pinnedPanel)];
  const arranging = useArranging(panels, solved, withKeys, !alone, pane);

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
      const to = moved ? landingAt(drag.panel, at.x, at.y, area, solved.panels) : null;
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
      to.target
        ? panels.layout.moved(drag.panel, { beside: to.target, edge: to.edge })
        : panels.layout.movedToEdgeOf(drag.panel, pane, to.edge),
    );
  };

  // A session's line carried over the pane: where it would be pinned lights up.
  const carried = (event: DragEvent) =>
    event.dataTransfer.types.includes(CARRIED) ? carriedSession() : null;
  const landing = (event: DragEvent, id: SessionId) => {
    const at = point(event);
    return landingAt(pinnedPanel(id), at.x, at.y, area, solved.panels);
  };
  const onDragOver = (event: DragEvent<HTMLDivElement>) => {
    const id = carried(event);
    if (!id) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    const to = landing(event, id);
    if (to?.rect !== dropping?.rect) setDropping(to);
  };
  const onDrop = (event: DragEvent<HTMLDivElement>) => {
    const id = carried(event);
    // Where it is let go, not where the last drag over lit.
    const to = id ? landing(event, id) : null;
    setDropping(null);
    carry(null);
    if (!id || !to) return;
    event.preventDefault();
    const panel = pinnedPanel(id);
    panels.keep(
      to.target
        ? panels.layout.pinning(id, { beside: to.target, edge: to.edge })
        : panels.layout
            .pinning(id, { beside: "session", edge: to.edge })
            .movedToEdgeOf(panel, [...pane, panel], to.edge),
    );
    app.actions.tell("Pinned it in a panel of its own.");
  };

  const placed = (panel: PanelName) => solved.panels.find((one) => one.panel === panel)?.rect;
  const style = (rect: Rect | undefined) =>
    rect
      ? { left: rect.x, top: rect.y, width: rect.width, height: rect.height }
      : { display: "none" as const };
  const dragging = drag?.kind === "panel" && drag.moved ? drag.panel : arranging;
  const lineDragged = drag?.kind === "line" ? lineKey(drag.divider) : null;

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: a session's line dropped here is pinned where it lands; the keys do the same with pin_session.
    <div
      ref={box}
      className={`panels ${drag ? "panels--dragging" : ""}`}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={() => {
        panels.show(null);
        setDrag(null);
      }}
      onDragOver={onDragOver}
      onDragLeave={(event) => {
        // Left for good, not for one of its own panels.
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(null);
      }}
      onDrop={onDrop}
    >
      {alone ? (
        <TerminalAlone session={session} />
      ) : (
        <>
          {agent ? (
            <SessionPanel
              session={agent}
              style={style(placed("session"))}
              lifted={dragging === "session"}
              onHeadDown={headDown("session")}
            />
          ) : null}
          {shell ? (
            <ShellPanel
              shell={shell}
              shown={session}
              style={style(placed("shell"))}
              lifted={dragging === "shell"}
              onHeadDown={headDown("shell")}
            />
          ) : null}
          {plugin ? (
            <PluginPanel
              panel={plugin}
              shown={session}
              style={style(placed("plugin"))}
              lifted={dragging === "plugin"}
              onHeadDown={headDown("plugin")}
            />
          ) : null}
          {pinned.map((id) => (
            <PinnedPanel
              key={id}
              session={id}
              style={style(placed(pinnedPanel(id)))}
              lifted={dragging === pinnedPanel(id)}
              onHeadDown={headDown(pinnedPanel(id))}
              onUnpin={() => panels.keep(panels.layout.unpinned(id))}
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
