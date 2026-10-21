// *Sessions* (the All list; design system: SessionTree): every project and
// its sessions, as the model lists them, in the sidebar, and beside it the
// selected session's pane, live, with its shell and plugins' panels. While
// Tickets or Settings is shown beside it instead, the list stays in the
// sidebar and its keys are theirs. The keys are the terminal UI's: ↑↓ (k j)
// move, ← → fold and unfold (or go out and in), `e` folds, ⏎ types in the
// session or folds the project, `T` a shell, `f` full screen, a plugin's key
// its panel, `d` the changes, `R` renames, `x` closes; on a session in a
// worktree, `c` its checks, `v` a review, `o` a draft pull request and `b`
// the pull request in the browser (src/session/byHand.ts).

import { useEffect } from "react";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import { Sidebar } from "../../app/Sidebar";
import { addProject, newSession } from "../../flows/actions";
import { labelOf } from "../../keys/keymap";
import { useActionKeys, useKeymap } from "../../keys/useKeymap";
import { toggleShell } from "../../panels/beside";
import { usePanelHandlers } from "../../panels/usePanelKeys";
import * as byHand from "../../session/byHand";
import { keysOf, type MenuItem } from "../../session/SessionMenu";
import { SessionPane, SidebarAway } from "../../session/SessionPane";
import { CloseSession, useSessionActions } from "../../session/useSessionActions";
import { standsBeside } from "../../state/selectors";
import type { Hint } from "../../state/ui";
import { Icon } from "../../ui/Icon";
import { RowDetail } from "./RowDetail";
import { SessionTree } from "./SessionTree";
import { useTree } from "./useTree";
import "./sessions.css";

const IN_TERMINAL: readonly Hint[] = [
  { key: "⇧Q", does: "give the keys back" },
  { key: "⌘\\", does: "fold the sidebar" },
];

function Empty() {
  return (
    <div className="mu-empty" role="status">
      <h2 className="mu-empty-title">No projects yet</h2>
      <p className="mu-empty-text">Each project you add shows here, with its sessions.</p>
    </div>
  );
}

export default function AllSessions({ active }: { active: boolean }) {
  const app = useApp();
  const actions = useActions();
  const tree = useTree();
  const { selected } = tree;
  const keysIn = useUi((state) => state.keysIn);
  const fullScreen = useUi((state) => state.fullScreen !== null);
  const loaded = useDaemon((state) => state.loaded);
  const link = useDaemon((state) => state.link.state);
  const map = useKeymap();
  const session = selected?.kind === "session" ? selected.id : undefined;
  const tools = useSessionActions(session);
  const hasPullRequest = useDaemon((state) =>
    Boolean(selected?.workspace && state.workspaces[selected.workspace]?.pull_request),
  );
  const isChat = useDaemon((state) => {
    const stored = session ? state.sessions[session] : undefined;
    return Boolean(stored && !standsBeside(stored));
  });
  const label = (action: string) => labelOf(map, action);
  const shellKey = label("shell");
  const fullKey = label("full_screen");
  const changesKey = label("show_changes");
  const renameKey = label("rename");
  const closeKey = label("close");
  const pullKey = label("pull_request_in_browser");

  /** The workspace a line works in: a session's, or a project's own checkout. */
  const workspaceOf = (): string | null => {
    if (!selected) return null;
    if (selected.workspace) return selected.workspace;
    const state = app.daemon.store.getState();
    return (
      Object.values(state.workspaces).find(
        (one) => one.project === selected.id && one.kind.kind === "checkout",
      )?.id ?? null
    );
  };
  const at = selected ? tree.rows.indexOf(selected) : -1;
  const move = (by: number) => {
    const next = tree.rows[Math.max(0, Math.min(tree.rows.length - 1, at + by))];
    if (next) tree.pick(next.id);
  };
  const open = () => {
    if (!selected) return;
    if (selected.kind === "project") return tree.fold(selected.id);
    // A worktree no session is in: one there, as the dialog starts it.
    if (selected.kind === "workspace") return newSession(app);
    const stored = app.daemon.store.getState().sessions[selected.id];
    if (!stored) return;
    actions.showPane("terminal");
    actions.showTerminal(stored.id);
    actions.keysTo("terminal");
  };
  const out = () => {
    if (!selected) return;
    if (selected.kind !== "project") return tree.pick(selected.project);
    if (!tree.folded.has(selected.id)) tree.fold(selected.id);
  };
  const into = () => {
    if (selected?.kind !== "project") return;
    if (tree.folded.has(selected.id)) return tree.fold(selected.id);
    const next = tree.rows[at + 1];
    if (next && next.kind !== "project") tree.pick(next.id);
  };
  const shellHere = () => {
    const workspace = workspaceOf();
    if (!workspace)
      return actions.tell(
        "This project has no checkout to open a shell in; select a session in a worktree.",
        true,
      );
    // Beside the checkout's chat where one works there, as its own shell icon opens it;
    // else on a line of its own.
    const sessions = app.daemon.store.getState().sessions;
    const chat = tree.rows.find((row) => {
      const session = row.kind === "session" ? sessions[row.id] : undefined;
      return session?.workspace === workspace && !standsBeside(session);
    });
    if (chat) {
      tree.pick(chat.id);
      void toggleShell(app, workspace, chat.id);
      return;
    }
    void toggleShell(app, workspace, null).then((shell) => {
      if (shell) tree.pick(shell);
    });
  };

  // The keys every list has, and the list's own, as the person set them.
  useKeys(
    active
      ? {
          ArrowDown: () => move(1),
          ArrowUp: () => move(-1),
          ArrowLeft: out,
          ArrowRight: into,
          Enter: open,
        }
      : {},
  );
  const panelsHere = usePanelHandlers({ session: null, workspace: workspaceOf() });
  useActionKeys(
    "sessions",
    active
      ? {
          move_down: () => move(1),
          move_up: () => move(-1),
          fold: () => {
            if (selected) tree.fold(selected.id);
          },
          checks: () => byHand.runChecks(app),
          review: () => byHand.review(app),
          open_pull_request: () => byHand.openPullRequest(app),
          pull_request_in_browser: () => byHand.browsePullRequest(app),
          ...(session
            ? {
                rename: tools.rename,
                close: tools.close,
              }
            : {
                // The pane's own keys answer on a session; on a project, the list's.
                ...panelsHere,
                shell: shellHere,
                full_screen: () => actions.tell("Select a session to open it full screen.", true),
              }),
        }
      : {},
  );

  const kind = selected?.kind ?? null;
  // The changes, while shown, say their own keys.
  const changesShown = useUi((state) => state.pane === "changes") && session !== undefined;
  useEffect(() => {
    if (changesShown || !active) return;
    if (keysIn === "terminal") return actions.setHints(IN_TERMINAL);
    if (fullScreen)
      return actions.setHints([
        { key: "⏎", does: "type in it" },
        { key: fullKey, does: "leave full screen" },
      ]);
    if (kind === "session")
      return actions.setHints([
        { key: "⏎", does: "type in it" },
        { key: shellKey, does: "shell" },
        { key: fullKey, does: "full screen" },
        { key: changesKey, does: "see changes" },
        ...(isChat ? [{ key: renameKey, does: "rename" }] : []),
        { key: closeKey, does: "close" },
        ...(hasPullRequest ? [{ key: pullKey, does: "pull request" }] : []),
      ]);
    if (kind === "workspace")
      return actions.setHints([
        { key: "⏎", does: "new session" },
        { key: shellKey, does: "shell" },
        { key: closeKey, does: "remove" },
      ]);
    if (kind)
      return actions.setHints([
        { key: "⏎", does: "fold or unfold" },
        { key: shellKey, does: "shell" },
      ]);
    actions.setHints([]);
  }, [
    actions,
    keysIn,
    fullScreen,
    kind,
    isChat,
    hasPullRequest,
    shellKey,
    fullKey,
    changesKey,
    renameKey,
    closeKey,
    pullKey,
    changesShown,
    active,
  ]);
  useEffect(() => {
    if (active) return () => actions.setHints([]);
  }, [actions, active]);

  const panelKeys: Record<string, string> = {};
  for (const bound of map.panels) {
    if (bound.chords.length)
      panelKeys[`plugin:${bound.action.plugin}/${bound.action.panel}`] = label(bound.name);
  }
  const menu: MenuItem[] = [
    ...(isChat ? [{ ...keysOf(map, "rename"), label: "Rename", does: tools.rename }] : []),
    { ...keysOf(map, "checks"), label: "Run the checks", does: () => byHand.runChecks(app) },
    { ...keysOf(map, "review"), label: "Review the change", does: () => byHand.review(app) },
    {
      ...keysOf(map, "open_pull_request"),
      label: "Open a draft pull request",
      does: () => byHand.openPullRequest(app),
    },
    ...(hasPullRequest
      ? [
          {
            ...keysOf(map, "pull_request_in_browser"),
            label: "Pull request in the browser",
            does: () => byHand.browsePullRequest(app),
          },
        ]
      : []),
    { ...keysOf(map, "close"), label: "Close", does: tools.close, danger: true },
  ];

  return (
    <>
      <Sidebar>
        <section className="sessions-column" aria-label="Sessions">
          <h1 className="app-hidden">Sessions</h1>
          {!loaded ? (
            <div className="mu-empty" role="status">
              <p className="mu-empty-text">
                {link === "down" ? "Muster's daemon is out of reach." : "Connecting to Muster…"}
              </p>
            </div>
          ) : tree.rows.length === 0 ? (
            <Empty />
          ) : (
            <SessionTree
              rows={tree.rows}
              folded={tree.folded}
              selected={selected?.id}
              resting={keysIn !== "list" || !active}
              renaming={tools.renaming ? tools : null}
              panelKeys={panelKeys}
              onPick={(id) => {
                // Picked beside Tickets or Settings, it is shown in their place.
                if (!active) actions.show("all");
                tree.pick(id);
                actions.keysTo("list");
              }}
              onFold={tree.fold}
            />
          )}
          <button
            type="button"
            className="mu-nav-item sessions-add"
            disabled={link !== "up"}
            onClick={() => addProject(app)}
          >
            <span className="mu-nav-icon">
              <Icon name="plus" size={16} />
            </span>
            Add a project
          </button>
        </section>
      </Sidebar>
      {active ? (
        <main className="app-screen">
          {session ? (
            <SessionPane session={session} menu={menu} />
          ) : selected ? (
            <RowDetail row={selected} shellKey={shellKey} />
          ) : (
            <section className="mu-pane app-pane sessions-detail" aria-label="No session">
              <header className="app-head" data-tauri-drag-region>
                <SidebarAway />
              </header>
              <p className="mu-meta sessions-detail-empty">
                The session you select shows here, live.
              </p>
            </section>
          )}
        </main>
      ) : null}
      <CloseSession actions={tools} />
    </>
  );
}
