// The sidebar: Kep beside the window's own buttons, and the way to put the
// sidebar away; a new session, the way to anything by typing and Tickets;
// the switch between what needs the person and every session, then the
// projects' heading with the way to add one, and the list itself under it,
// sessions by project either way (the list screens draw it: QueueScreen,
// AllSessions); Settings at its foot. It is the one list of the window: the
// session or project selected in it fills the rest.

import type { ReactNode } from "react";
import { addProject, newSession } from "../flows/actions";
import { useKeyLabel } from "../keys/useKeymap";
import { chats } from "../state/selectors";
import type { ListId } from "../state/ui";
import { IconButton, Kbd } from "../ui/controls";
import { Icon, type IconName } from "../ui/Icon";
import { KepFace, type Mood } from "../ui/Kep";
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
    <button
      type="button"
      className="mu-icon-btn app-sidebar-toggle"
      aria-label={hidden ? "Show the sidebar" : "Hide the sidebar"}
      title={`${hidden ? "Show" : "Hide"} the sidebar (⌘\\)`}
      // Full screen brings the sidebar back as it leaves.
      onClick={() => (fullScreen ? actions.fillWindow(null) : actions.foldSidebar())}
    >
      <Icon name="sidebar" size={16} />
    </button>
  );
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
    <button
      type="button"
      className="mu-nav-item"
      aria-current={current ? "page" : undefined}
      disabled={disabled}
      onClick={onClick}
    >
      <span className="mu-nav-icon">
        <Icon name={icon} size={16} />
      </span>
      {label}
      {keys ? <Kbd className="app-sidebar-key">{keys}</Kbd> : null}
    </button>
  );
}

function ListSwitch() {
  const actions = useActions();
  const list = useUi((state) => state.list);
  const waiting = useDaemon((state) => state.derived.counts.waiting);
  const all = useDaemon((state) => chats(state).length);
  const segment = (id: ListId, label: string, count: number, tone: string) => (
    <button
      type="button"
      className="mu-seg-item app-lists-item"
      aria-pressed={list === id}
      onClick={() => actions.show(id)}
    >
      {label}
      {count > 0 ? <span className={`app-lists-count ${tone}`}>{count}</span> : null}
    </button>
  );
  return (
    // biome-ignore lint/a11y/useSemanticElements: a row of toggle buttons, as Segmented's.
    <div className="mu-seg app-lists" role="group" aria-label="Sessions listed">
      {segment("needs-you", "Needs you", waiting, "app-lists-count--waiting")}
      {segment("all", "All", all, "")}
    </div>
  );
}

/** The projects' heading, and the way to add one. */
function ProjectsHead({ linked }: { linked: boolean }) {
  const app = useApp();
  const key = useKeyLabel("add_project");
  return (
    <div className="app-projects-head">
      <span className="app-projects-title">Projects</span>
      <IconButton
        icon="plus"
        size="sm"
        label={`Add a project (${key})`}
        disabled={!linked}
        onClick={() => addProject(app)}
      />
    </div>
  );
}

export function Sidebar({ children }: { children: ReactNode }) {
  const app = useApp();
  const actions = useActions();
  const mood = useMood();
  const folded = useUi((state) => state.sidebarFolded);
  const screen = useUi((state) => state.screen);
  const linked = useDaemon((state) => state.link.state === "up");
  return (
    <nav className="mu-nav app-sidebar" aria-label="Sessions" hidden={folded}>
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
      <div className="app-sidebar-list">{children}</div>
      <SideButton
        icon="gear"
        label="Settings"
        keys="⌘,"
        current={screen === "settings"}
        onClick={() => actions.show("settings")}
      />
    </nav>
  );
}
