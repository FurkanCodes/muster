// The sidebar: Kep beside the window's own buttons, and the way to put the
// sidebar away; a new session, the way to anything by typing and Tickets;
// the switch between what needs the person and every session, then the
// projects' heading with the way to add one, and the list itself under it,
// sessions by project either way (the list screens draw it: QueueScreen,
// AllSessions); Settings at its foot. It is the one list of the window: the
// session or project selected in it fills the rest.

import {
  type MouseEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { addProject, cloneProject, newSession } from "../flows/actions";
import { useKeymap } from "../keys/useKeymap";
import { keep, kept } from "../state/kept";
import { chats } from "../state/selectors";
import type { ListId } from "../state/ui";
import { IconButton, Kbd, Segmented } from "../ui/controls";
import { Icon, type IconName } from "../ui/Icon";
import { KepFace, type Mood } from "../ui/Kep";
import { ActionMenu, keysOf } from "../ui/Menu";
import { NavItem } from "../ui/Nav";
import { useActions, useApp, useDaemon, useUi } from "./context";

function useMood(): Mood {
  return useDaemon((state): Mood => {
    if (state.link.state === "down") return "away";
    if (state.derived.counts.waiting > 0) return "barking";
    if (state.derived.counts.working > 0 || state.link.state === "connecting") return "watching";
    return "asleep";
  });
}

/** The button that puts the sidebar away, or brings it back from the pane's head. */
export function SidebarToggle() {
  const actions = useActions();
  const folded = useUi((state) => state.sidebarFolded);
  const fullScreen = useUi((state) => state.fullScreen !== null);
  const hidden = folded || fullScreen;
  return (
    <IconButton
      icon="sidebar"
      className="app-sidebar-toggle"
      label={hidden ? "Show the sidebar" : "Hide the sidebar"}
      title={`${hidden ? "Show" : "Hide"} the sidebar (⌘\\)`}
      // Full screen brings the sidebar back as it leaves.
      onClick={() => (fullScreen ? actions.fillWindow(null) : actions.foldSidebar())}
    />
  );
}

/** The window's own buttons sit over the pane's head while the sidebar is away: room for them, and the way back. */
export function SidebarAway() {
  const away = useUi((state) => state.sidebarFolded || state.fullScreen !== null);
  return away ? (
    <span className="app-head-away">
      <SidebarToggle />
    </span>
  ) : null;
}

/**
 * Over a screen with no head of its own (Tickets, Settings), while the
 * sidebar is away: a row clear of the window's buttons, and the way back.
 */
export function SidebarAwayRow() {
  const away = useUi((state) => state.sidebarFolded || state.fullScreen !== null);
  return away ? (
    <div className="app-away-row" data-tauri-drag-region>
      <SidebarToggle />
    </div>
  ) : null;
}

function SideButton({
  icon,
  label,
  keys,
  current = false,
  disabled = false,
  onClick,
}: {
  icon: IconName;
  label: string;
  keys?: string;
  current?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <NavItem
      icon={<Icon name={icon} size={16} />}
      current={current}
      disabled={disabled}
      onClick={onClick}
    >
      {label}
      {keys ? <Kbd className="app-sidebar-key">{keys}</Kbd> : null}
    </NavItem>
  );
}

function ListSwitch() {
  const actions = useActions();
  const list = useUi((state) => state.list);
  const waiting = useDaemon((state) => state.derived.counts.waiting);
  const all = useDaemon((state) => chats(state).length);
  return (
    <Segmented<ListId>
      label="Sessions listed"
      className="app-lists"
      segments={[
        { value: "needs-you", label: "Needs you", count: waiting, tone: "waiting" },
        { value: "all", label: "All", count: all },
      ]}
      value={list}
      onChange={(id) => actions.show(id)}
    />
  );
}

/** The projects' heading, and the ways to add one: a folder of the Mac's, or a clone from GitHub. */
function ProjectsHead({ linked }: { linked: boolean }) {
  const app = useApp();
  const map = useKeymap();
  return (
    <div className="app-projects-head">
      <span className="app-projects-title">Projects</span>
      <ActionMenu
        label="Add a project"
        items={[
          { ...keysOf(map, "add_project"), label: "Open a folder…", does: () => addProject(app) },
          {
            ...keysOf(map, "clone_project"),
            label: "Clone from GitHub…",
            does: () => cloneProject(app),
          },
        ]}
        trigger={<IconButton icon="plus" size="sm" label="Add a project" disabled={!linked} />}
      />
    </div>
  );
}

/** The sidebar's width as the window opens, and the bounds a drag keeps it in. */
const WIDTH_KEY = "muster.sidebar-width";
export const SIDEBAR_WIDTH = { first: 288, least: 220, most: 420 } as const;

function inBounds(width: number): number {
  return Math.round(Math.max(SIDEBAR_WIDTH.least, Math.min(SIDEBAR_WIDTH.most, width)));
}

/** The width the person left it at, kept as the theme is; the first width where none was kept. */
function keptWidth(): number {
  const width = Number(kept(WIDTH_KEY));
  return Number.isFinite(width) && width > 0 ? inBounds(width) : SIDEBAR_WIDTH.first;
}

/**
 * The sidebar's right edge: dragged, the sidebar follows the pointer and
 * keeps its width once let go; a double-click puts it back as it was at first.
 */
function SidebarEdge({
  width,
  onWidth,
}: {
  width: number;
  onWidth: (width: number, done: boolean) => void;
}) {
  const from = useRef<{ x: number; width: number } | null>(null);
  const [dragged, setDragged] = useState(false);
  // A drag that ends however it ends (let go, or the capture lost) keeps the width.
  const end = () => {
    if (!from.current) return;
    from.current = null;
    setDragged(false);
    delete document.documentElement.dataset.resizing;
    onWidth(width, true);
  };
  useEffect(() => () => void delete document.documentElement.dataset.resizing, []);
  return (
    // biome-ignore lint/a11y/useSemanticElements: a splitter dragged by the pointer, which an <hr> cannot be.
    <div
      className={`app-sidebar-edge ${dragged ? "is-dragged" : ""}`}
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      aria-valuemin={SIDEBAR_WIDTH.least}
      aria-valuemax={SIDEBAR_WIDTH.most}
      aria-valuenow={width}
      title="Drag to resize; double-click to put back"
      onPointerDown={(event: PointerEvent<HTMLDivElement>) => {
        if (event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.setPointerCapture(event.pointerId);
        from.current = { x: event.clientX, width };
        setDragged(true);
        document.documentElement.dataset.resizing = "";
      }}
      onPointerMove={(event) => {
        if (from.current) onWidth(from.current.width + event.clientX - from.current.x, false);
      }}
      onPointerUp={end}
      onLostPointerCapture={end}
      onDoubleClick={() => onWidth(SIDEBAR_WIDTH.first, true)}
    />
  );
}

/** The sidebar's width, and the way to change it: kept once a change is done. */
function useSidebarWidth(): [number, (width: number, done: boolean) => void] {
  const [width, setWidth] = useState(keptWidth);
  return [
    width,
    (wanted, done) => {
      const next = inBounds(wanted);
      setWidth(next);
      if (done) keep(WIDTH_KEY, next === SIDEBAR_WIDTH.first ? null : String(next));
    },
  ];
}

/**
 * A right-click on a line of the list selects it, as a click does, and opens
 * its menu where the pointer is: a session's is the one under its ⋯.
 */
function useLineMenu(): (event: MouseEvent<HTMLElement>) => void {
  const app = useApp();
  return (event) => {
    const line = (event.target as Element).closest<HTMLElement>(
      '[role="option"], [role="treeitem"]',
    );
    if (!line) return;
    event.preventDefault();
    // The list has the keys, as after a click: the menu gives them back to it.
    line.closest<HTMLElement>('[role="listbox"], [role="tree"]')?.focus({ preventScroll: true });
    line.click();
    const ui = app.ui.getState();
    const id = ui.selected[ui.screen];
    if (id && app.daemon.store.getState().sessions[id])
      app.actions.openMenuAt({ session: id, x: event.clientX, y: event.clientY });
  };
}

export function Sidebar({ children }: { children: ReactNode }) {
  const app = useApp();
  const lineMenu = useLineMenu();
  const [width, setWidth] = useSidebarWidth();
  const actions = useActions();
  const mood = useMood();
  const folded = useUi((state) => state.sidebarFolded);
  const screen = useUi((state) => state.screen);
  const linked = useDaemon((state) => state.link.state === "up");
  return (
    <nav className="mu-nav app-sidebar" aria-label="Sessions" hidden={folded} style={{ width }}>
      <div className="app-sidebar-top" data-tauri-drag-region>
        <KepFace mood={mood} />
        <span className="mu-spacer" data-tauri-drag-region />
        <SidebarToggle />
      </div>
      <SideButton
        icon="plus"
        label="New session"
        keys="⌘N"
        disabled={!linked}
        onClick={() => newSession(app)}
      />
      <SideButton
        icon="search"
        label="Search"
        keys="⌘K"
        disabled={!linked}
        onClick={() => actions.openPalette(true)}
      />
      <SideButton
        icon="ticket"
        label="Tickets"
        current={screen === "tickets"}
        onClick={() => actions.show("tickets")}
      />
      <ListSwitch />
      <ProjectsHead linked={linked} />
      {/* biome-ignore lint/a11y/noStaticElementInteractions: the right-click is the pointer's way to a line's menu; ⋯ and ⌘K are the keys'. */}
      <div className="app-sidebar-list" onContextMenu={lineMenu}>
        {children}
      </div>
      <SideButton
        icon="gear"
        label="Settings"
        keys="⌘,"
        current={screen === "settings"}
        onClick={() => actions.show("settings")}
      />
      <SidebarEdge width={width} onWidth={setWidth} />
    </nav>
  );
}
