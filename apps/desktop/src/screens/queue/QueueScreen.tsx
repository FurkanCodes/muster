// *Needs you*, *Not seen* and *Working* (design system: Queue): the queue
// column, and beside it the selected session's pane, live. The keys are the
// terminal UI's: ↑↓ (or k j) move, ⏎ gives the session's terminal the keys
// and ⇧Q gives them back, `l` goes to the session that needed the person
// last, `d` its changes, `z` puts its newest turn back, `R` renames it in
// place, `x` closes it, `b` its pull request.

import { type ReactNode, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { type KeyHandlers, useKeys } from "../../app/keys";
import type { SessionId } from "../../ipc/generated/SessionId";
import { agentOf } from "../../lib/agents";
import type { MenuItem } from "../../session/SessionMenu";
import { SessionPane } from "../../session/SessionPane";
import {
  CloseSession,
  RenameSession,
  type SessionActions,
  useSessionActions,
} from "../../session/useSessionActions";
import { standsBeside } from "../../state/selectors";
import type { Hint } from "../../state/ui";
import { lastNeeded } from "./history";
import { optionId, QueueEntry } from "./QueueEntry";
import { holdPlace, type QueueArea, queueIds, type Section, sectionOf } from "./queue";
import "./queue.css";

const HEADS: Record<QueueArea, [title: string, help: string]> = {
  "needs-you": ["Needs you", "Oldest first. Opening one clears it."],
  "not-seen": ["Not seen", "Finished while you looked elsewhere, oldest first."],
  working: ["Working", "Agents at work now, project by project."],
};

const SECTIONS: Record<Section, string> = {
  asking: "Asking you",
  news: "Finished, not seen",
  working: "Working",
};

/** The list as shown and its selection, holding the selected one's place (`holdPlace`). */
function useShownQueue(area: QueueArea) {
  const project = useUi((state) => state.project);
  const chosen = useUi((state) => state.selected[area]);
  const list = useDaemon(useShallow((state) => queueIds(state, area, project)));
  // Kept in its place only while it is not at work again, and in the project shown.
  const stays = useDaemon((state) => {
    const session = chosen ? state.sessions[chosen] : undefined;
    if (!session || area === "working" || session.status.state === "running") return false;
    return !project || state.workspaces[session.workspace]?.project === project;
  });
  const [held, setHeld] = useState(() => ({
    list,
    chosen,
    stays,
    ...holdPlace(list, [], chosen, false),
  }));
  let shown = held;
  if (held.list !== list || held.chosen !== chosen || held.stays !== stays) {
    // Worked out from what was shown last, as React keeps it between renders.
    shown = { list, chosen, stays, ...holdPlace(list, held.ids, chosen, stays) };
    setHeld(shown);
  }
  return { ids: shown.ids, selected: shown.selected, chosen };
}

function hintsFor(
  section: Section | null,
  canPutBack: boolean,
  hasPullRequest: boolean,
): readonly Hint[] {
  if (!section) return [{ key: "l", does: "last needed you" }];
  return [
    { key: "⏎", does: section === "asking" ? "answer" : "open" },
    { key: "d", does: "see changes" },
    ...(canPutBack ? [{ key: "z", does: "put back" }] : []),
    { key: "R", does: "rename" },
    { key: "x", does: "close" },
    ...(hasPullRequest ? [{ key: "b", does: "pull request" }] : []),
    { key: "l", does: "last needed you" },
  ];
}

const IN_TERMINAL: readonly Hint[] = [
  { key: "⇧Q", does: "give the keys back" },
  { key: "⌘\\", does: "fold the sidebar" },
];

export function QueueScreen({ area }: { area: QueueArea }) {
  const app = useApp();
  const actions = useActions();
  const { ids, selected, chosen } = useShownQueue(area);
  const keysIn = useUi((state) => state.keysIn);
  const loaded = useDaemon((state) => state.loaded);
  const link = useDaemon((state) => state.link.state);
  const sections = useDaemon(
    useShallow((state) =>
      ids.map((id) => {
        const session = state.sessions[id];
        return session ? sectionOf(session) : "news";
      }),
    ),
  );
  const tools = useSessionActions(selected);
  const workspace = useDaemon((state) =>
    selected ? state.sessions[selected]?.workspace : undefined,
  );
  const newestTurn = useDaemon((state) =>
    workspace ? (state.workspaces[workspace]?.timeline ?? null) : null,
  );
  const busy = useDaemon((state) =>
    Object.values(state.sessions).some(
      (session) =>
        session.workspace === workspace &&
        session.status.state === "running" &&
        !standsBeside(session),
    ),
  );
  const hasPullRequest = useDaemon((state) =>
    Boolean(workspace && state.workspaces[workspace]?.pull_request),
  );
  const [title, help] = HEADS[area];
  const at = selected ? ids.indexOf(selected) : -1;
  const section = at >= 0 ? (sections[at] ?? null) : null;

  // The selection the list holds is the screen's (`ui.selected`).
  useEffect(() => {
    if (selected !== chosen) actions.select(area, selected);
  }, [actions, area, selected, chosen]);

  // Back from a session that waits no more, the queue moves on, as in the terminal UI.
  const keysWere = useRef(keysIn);
  useEffect(() => {
    const left = keysWere.current === "terminal" && keysIn === "list";
    keysWere.current = keysIn;
    if (!left || area === "working" || !selected) return;
    const session = app.daemon.store.getState().sessions[selected];
    if (session && session.status.state !== "needs_you" && !session.unseen) {
      const first = queueIds(app.daemon.store.getState(), area, app.ui.getState().project)[0];
      actions.select(area, first);
    }
  }, [app, actions, area, keysIn, selected]);

  useEffect(() => {
    actions.setHints(
      keysIn === "terminal" ? IN_TERMINAL : hintsFor(section, newestTurn !== null, hasPullRequest),
    );
  }, [actions, keysIn, section, newestTurn, hasPullRequest]);
  useEffect(() => () => actions.setHints([]), [actions]);

  const pick = (id: SessionId) => {
    actions.select(area, id);
    actions.keysTo("list");
  };
  const move = (by: number) => {
    const next = ids[Math.max(0, Math.min(ids.length - 1, (at < 0 ? 0 : at) + by))];
    if (next) pick(next);
  };
  const open = () => {
    if (!selected) return;
    const session = app.daemon.store.getState().sessions[selected];
    if (!session) return;
    actions.showPane("terminal");
    actions.showTerminal(selected);
    actions.keysTo("terminal");
    actions.tell(`Typing in ${session.title}. Every key goes to ${agentOf(session.harness).name}.`);
  };
  const putBack = () => {
    if (!workspace || newestTurn === null)
      return actions.tell("No turn is kept for this workspace yet.", true);
    if (busy) return actions.tell("Wait for its turn to end.", true);
    actions.showTurn(workspace, newestTurn, "put-back");
  };
  const goToLast = () => {
    const last = lastNeeded(app.daemon);
    if (!last) return actions.tell("No session has needed you since Muster started.", true);
    if (ids.includes(last)) return pick(last);
    // Not in this list (seen since, or another project's): where every session is.
    actions.select("all", last);
    actions.show("all");
  };
  const keys: KeyHandlers = {
    ArrowDown: () => move(1),
    ArrowUp: () => move(-1),
    j: () => move(1),
    k: () => move(-1),
    Enter: open,
    d: () => {
      if (selected) actions.showPane("changes");
    },
    z: putBack,
    R: tools.rename,
    x: tools.close,
    b: tools.browser,
    l: goToLast,
  };
  useKeys(keys);

  const menu: MenuItem[] = [
    { key: "R", label: "Rename", does: tools.rename },
    ...(hasPullRequest
      ? [{ key: "b", label: "Pull request in the browser", does: tools.browser }]
      : []),
    { key: "x", label: "Close", does: tools.close, danger: true },
  ];
  const resting = keysIn !== "list";
  const working = sections.filter((one) => one === "working").length;
  const nothingWaits = area === "needs-you" && working === sections.length;

  return (
    <>
      <section className="queue-column" aria-label={title}>
        <div className="mu-queue-head">
          <h1 className="mu-queue-title">{title}</h1>
          <span className="mu-meta">{help}</span>
        </div>
        {!loaded ? (
          <div className="mu-empty" role="status">
            <p className="mu-empty-text">
              {link === "down" ? "Muster's daemon is out of reach." : "Connecting to Muster…"}
            </p>
          </div>
        ) : (
          <div
            className="mu-queue queue-list"
            role="listbox"
            tabIndex={0}
            aria-label={`${title} sessions`}
            aria-activedescendant={selected ? optionId(selected) : undefined}
          >
            {nothingWaits ? <AllClear area={area} working={working} /> : null}
            {area !== "needs-you" && ids.length === 0 ? <Empty area={area} /> : null}
            {ids.map((id, index) => {
              const own = sections[index] ?? "news";
              const head = area === "needs-you" && own !== sections[index - 1];
              return (
                <QueueRowSlot
                  key={id}
                  head={head ? SECTIONS[own] : null}
                  section={own}
                  renaming={tools.renaming === id ? tools : null}
                >
                  <QueueEntry
                    id={id}
                    area={area}
                    selected={id === selected}
                    resting={resting}
                    onPick={pick}
                  />
                </QueueRowSlot>
              );
            })}
          </div>
        )}
      </section>
      {selected ? <SessionPane session={selected} menu={menu} /> : <NoneSelected area={area} />}
      <CloseSession actions={tools} />
    </>
  );
}

/** A row of the list, under its section's label where a section begins, or its name being edited. */
function QueueRowSlot({
  head,
  section,
  renaming,
  children,
}: {
  head: string | null;
  section: Section;
  renaming: SessionActions | null;
  children: ReactNode;
}) {
  return (
    <>
      {head ? (
        <div className={`mu-section mu-section--${section}`} role="presentation">
          {head}
        </div>
      ) : null}
      {renaming ? <RenameSession actions={renaming} /> : children}
    </>
  );
}

function AllClear({ area, working }: { area: QueueArea; working: number }) {
  if (area !== "needs-you") return null;
  return (
    <div className="mu-empty queue-clear" role="presentation">
      <h2 className="mu-empty-title queue-clear-title">All clear</h2>
      <p className="mu-empty-text">
        {working === 0
          ? "No agent is at work. Muster will show them here when they need you."
          : `${working} ${working === 1 ? "agent" : "agents"} at work. Muster will show them here when they need you.`}
      </p>
    </div>
  );
}

function Empty({ area }: { area: QueueArea }) {
  return (
    <div className="mu-empty" role="presentation">
      <h2 className="mu-empty-title">{area === "not-seen" ? "All seen" : "No agent at work"}</h2>
      <p className="mu-empty-text">
        {area === "not-seen"
          ? "Agents that finish while you look elsewhere come here until you open them."
          : "Agents show here while they work, with what they are doing."}
      </p>
    </div>
  );
}

/** The pane's place while nothing is selected: the window keeps its shape. */
function NoneSelected({ area }: { area: QueueArea }) {
  return (
    <section className="mu-pane app-pane session-pane queue-none" aria-label="No session">
      <p className="mu-meta">
        {area === "working"
          ? "A session at work shows here, live."
          : "The session you select shows here, live."}
      </p>
    </section>
  );
}
