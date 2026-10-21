// The screens, and the projects to narrow them by (design system: Sidebar).

import type { ReactNode } from "react";
import { useShallow } from "zustand/react/shallow";
import { chats, projectColour, projectsInOrder } from "../state/selectors";
import type { ScreenId } from "../state/ui";
import { Spinner } from "../ui/controls";
import { Icon } from "../ui/Icon";
import { useMediaQuery } from "../ui/useMediaQuery";
import { useActions, useDaemon, useUi } from "./context";

function NavItem({
  screen,
  label,
  icon,
  count = 0,
  waiting = false,
  folded,
}: {
  screen: ScreenId;
  label: string;
  icon: ReactNode;
  count?: number;
  waiting?: boolean;
  folded: boolean;
}) {
  const actions = useActions();
  const current = useUi((state) => state.screen === screen);
  return (
    <button
      type="button"
      className="mu-nav-item"
      aria-current={current ? "page" : undefined}
      title={folded ? label : undefined}
      aria-label={folded ? label : undefined}
      onClick={() => actions.show(screen)}
    >
      <span className="mu-nav-icon">{icon}</span>
      {folded ? null : (
        <>
          {label}
          {count > 0 ? (
            <span className={`mu-nav-count ${waiting ? "mu-nav-count--waiting" : ""}`}>
              {count}
            </span>
          ) : null}
        </>
      )}
    </button>
  );
}

/** One project; each reads only its own name, colour and count, as numbers and strings. */
function ProjectItem({ id }: { id: string }) {
  const actions = useActions();
  const narrowed = useUi((state) => state.project === id);
  const name = useDaemon((state) => state.projects[id]?.name ?? "");
  const colour = useDaemon((state) => projectColour(state, id));
  const sessions = useDaemon(
    (state) =>
      chats(state).filter((session) => state.workspaces[session.workspace]?.project === id).length,
  );
  return (
    <button
      type="button"
      className="mu-nav-item"
      aria-pressed={narrowed}
      onClick={() => actions.narrow(id)}
    >
      <span className="mu-nav-icon">
        <span className={`mu-swatch mu-p${colour}`} />
      </span>
      <span className="app-sidebar-project">{name}</span>
      <span className="mu-nav-count">{sessions}</span>
    </button>
  );
}

function Projects() {
  const actions = useActions();
  const ids = useDaemon(useShallow((state) => projectsInOrder(state).map((project) => project.id)));
  const linked = useDaemon((state) => state.link.state === "up");
  return (
    <>
      <div className="mu-nav-label">Projects</div>
      {ids.map((id) => (
        <ProjectItem key={id} id={id} />
      ))}
      <button
        type="button"
        className="mu-nav-item"
        disabled={!linked}
        onClick={() => actions.openFlow({ kind: "add-project" })}
      >
        <span className="mu-nav-icon">
          <Icon name="plus" />
        </span>
        Add a project
      </button>
    </>
  );
}

const WAITING = <span className="app-nav-waiting">▲</span>;
const NEWS = <span className="mu-dot" />;
/** Working's mark spins only while an agent works: an idle window draws nothing (`ARC-25`). */
function Working({ working }: { working: boolean }) {
  return (
    <span className="app-nav-working">
      <Spinner still={!working} />
    </span>
  );
}
const ALL = <span>≡</span>;
const TICKETS = <span>#</span>;
const SETTINGS = <Icon name="gear" size={16} />;

export function Sidebar() {
  // Under 1100 wide (sidebar 220, queue 360, detail 520) it folds by itself;
  // ⌘\ folds and unfolds it at any width (design system: layout).
  const toggled = useUi((state) => state.sidebarFolded);
  const narrow = useMediaQuery("(max-width: 1100px)");
  const folded = narrow !== toggled;
  const counts = useDaemon((state) => state.derived.counts);
  const notSeen = useDaemon(
    (state) =>
      chats(state).filter((session) => session.unseen && session.status.state !== "needs_you")
        .length,
  );
  const all = useDaemon((state) => chats(state).length);
  return (
    <nav className={`mu-nav app-sidebar ${folded ? "mu-nav--folded" : ""}`} aria-label="Screens">
      <NavItem
        screen="needs-you"
        label="Needs you"
        icon={WAITING}
        count={counts.waiting}
        waiting
        folded={folded}
      />
      <NavItem screen="not-seen" label="Not seen" icon={NEWS} count={notSeen} folded={folded} />
      <NavItem
        screen="working"
        label="Working"
        icon={<Working working={counts.working > 0} />}
        count={counts.working}
        folded={folded}
      />
      <NavItem screen="all" label="All sessions" icon={ALL} count={all} folded={folded} />
      {folded ? null : <Projects />}
      <span className="app-sidebar-gap" />
      <NavItem screen="tickets" label="Tickets" icon={TICKETS} folded={folded} />
      <NavItem screen="settings" label="Settings" icon={SETTINGS} folded={folded} />
    </nav>
  );
}
