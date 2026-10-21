// *Needs you* (design system: Queue): the queue column, asking first, then
// finished not seen, then at work, and beside it the selected session's pane, live. The keys are the
// terminal UI's, as the person set them (src/keys): ↑↓ (or k j) move, ⏎
// gives the session's terminal the keys and ⇧Q gives them back, `l` goes
// to the session that needed the person last, `d` its changes (the pane's),
// `z` puts its newest turn back, `R` renames it in place, `x` closes it, `b`
// its pull request in the browser.

import { type ReactNode, useEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import { Narrowed } from "../../app/Narrowed";
import type { SessionId } from "../../ipc/generated/SessionId";
import { type Keymap, labelOf } from "../../keys/keymap";
import { useActionKeys, useBorrowedKeys, useKeymap } from "../../keys/useKeymap";
import { keysOf, type MenuItem } from "../../session/SessionMenu";
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
import { holdPlace, queueIds, type Section, sectionOf } from "./queue";
import "./queue.css";

const TITLE = "Needs you";
const AREA = "needs-you";

const SECTIONS: Record<Section, string> = {
  asking: "Asking you",
  news: "Finished, not seen",
  working: "Working",
};

/** The list as shown and its selection, holding the selected one's place (`holdPlace`). */
function useShownQueue() {
  const project = useUi((state) => state.project);
  const chosen = useUi((state) => state.selected[AREA]);
  const list = useDaemon(useShallow((state) => queueIds(state, project)));
  // Kept in its place only while it is not at work again, and in the project shown.
  const stays = useDaemon((state) => {
    const session = chosen ? state.sessions[chosen] : undefined;
    if (!session || session.status.state === "running") return false;
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
  map: Keymap,
  section: Section | null,
  canPutBack: boolean,
  hasPullRequest: boolean,
): readonly Hint[] {
  const key = (action: string) => labelOf(map, action);
  const hints: Hint[] = section
    ? [
        { key: "⏎", does: section === "asking" ? "answer" : "open" },
        { key: key("show_changes"), does: "see changes" },
        ...(canPutBack ? [{ key: key("put_back"), does: "put back" }] : []),
        { key: key("rename"), does: "rename" },
        { key: key("close"), does: "close" },
        ...(hasPullRequest ? [{ key: key("pull_request_in_browser"), does: "pull request" }] : []),
        { key: key("last_alarm"), does: "last needed you" },
      ]
    : [{ key: key("last_alarm"), does: "last needed you" }];
  // A key the person took from its action is no hint.
  return hints.filter((hint) => hint.key !== "none");
}

const IN_TERMINAL: readonly Hint[] = [
  { key: "⇧Q", does: "give the keys back" },
  { key: "⌘\\", does: "fold the sidebar" },
];

export function QueueScreen() {
  const app = useApp();
  const actions = useActions();
  const { ids, selected, chosen } = useShownQueue();
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
  const at = selected ? ids.indexOf(selected) : -1;
  const section = at >= 0 ? (sections[at] ?? null) : null;

  // The selection the list holds is the screen's (`ui.selected`).
  useEffect(() => {
    if (selected !== chosen) actions.select(AREA, selected);
  }, [actions, selected, chosen]);

  // Back from a session that waits no more, the queue moves on, as in the terminal UI.
  const keysWere = useRef(keysIn);
  useEffect(() => {
    const left = keysWere.current === "terminal" && keysIn === "list";
    keysWere.current = keysIn;
    if (!left || !selected) return;
    const session = app.daemon.store.getState().sessions[selected];
    if (session && session.status.state !== "needs_you" && !session.unseen) {
      const first = queueIds(app.daemon.store.getState(), app.ui.getState().project)[0];
      actions.select(AREA, first);
    }
  }, [app, actions, keysIn, selected]);

  const map = useKeymap();
  // The changes, while shown, say their own keys.
  const changesShown = useUi((state) => state.pane === "changes") && selected !== undefined;
  useEffect(() => {
    if (changesShown) return;
    actions.setHints(
      keysIn === "terminal"
        ? IN_TERMINAL
        : hintsFor(map, section, newestTurn !== null, hasPullRequest),
    );
  }, [actions, map, keysIn, section, newestTurn, hasPullRequest, changesShown]);
  useEffect(() => () => actions.setHints([]), [actions]);

  const pick = (id: SessionId) => {
    actions.select(AREA, id);
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
  useKeys({ ArrowDown: () => move(1), ArrowUp: () => move(-1), Enter: open });
  // The pane answers `d` (show_changes) and its panels' keys.
  useActionKeys("needs_you", {
    move_down: () => move(1),
    move_up: () => move(-1),
    rename: tools.rename,
    close: tools.close,
    last_alarm: goToLast,
  });
  useBorrowedKeys("needs_you", { put_back: putBack, pull_request_in_browser: tools.browser });

  const menu: MenuItem[] = [
    { ...keysOf(map, "rename"), label: "Rename", does: tools.rename },
    ...(hasPullRequest
      ? [
          {
            ...keysOf(map, "pull_request_in_browser"),
            label: "Pull request in the browser",
            does: tools.browser,
          },
        ]
      : []),
    { ...keysOf(map, "close"), label: "Close", does: tools.close, danger: true },
  ];
  const resting = keysIn !== "list";
  const working = sections.filter((one) => one === "working").length;
  const nothingWaits = working === sections.length;

  return (
    <>
      <section className="queue-column" aria-label={TITLE}>
        <div className="mu-queue-head">
          <h1 className="mu-queue-title">{TITLE}</h1>
          <span className="mu-meta">Oldest first. Opening one clears it.</span>
          <Narrowed />
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
            aria-label={`${TITLE} sessions`}
            aria-activedescendant={selected ? optionId(selected) : undefined}
          >
            {nothingWaits ? <AllClear working={working} /> : null}
            {ids.map((id, index) => {
              const own = sections[index] ?? "news";
              const head = own !== sections[index - 1];
              return (
                <QueueRowSlot
                  key={id}
                  head={head ? SECTIONS[own] : null}
                  section={own}
                  renaming={tools.renaming === id ? tools : null}
                >
                  <QueueEntry id={id} selected={id === selected} resting={resting} onPick={pick} />
                </QueueRowSlot>
              );
            })}
          </div>
        )}
      </section>
      {selected ? <SessionPane session={selected} menu={menu} /> : <NoneSelected />}
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

function AllClear({ working }: { working: number }) {
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

/** The pane's place while nothing is selected: the window keeps its shape. */
function NoneSelected() {
  return (
    <section className="mu-pane app-pane session-pane queue-none" aria-label="No session">
      <p className="mu-meta">The session you select shows here, live.</p>
    </section>
  );
}
