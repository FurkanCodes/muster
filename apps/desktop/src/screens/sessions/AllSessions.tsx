// *All sessions* (the AllSessions board; design system: SessionTree): every
// project, its worktrees and their sessions, as the model lists them, and
// beside the tree the selected session's pane, live, with its shell and
// plugins' panels. The keys are the terminal UI's: ↑↓ (k j) move, ← → fold
// and unfold (or go out and in), `e` folds, ⏎ types in the session or folds
// the line, `T` a shell, `f` full screen, a plugin's key its panel, `d` the
// changes, `R` renames, `x` closes; on a worktree or a session in one, `c`
// its checks, `v` a review, `o` a draft pull request and `b` the pull
// request in the browser (src/session/byHand.ts).

import { useEffect } from "react";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { useKeys } from "../../app/keys";
import { labelOf } from "../../keys/keymap";
import { useActionKeys, useKeymap } from "../../keys/useKeymap";
import { agentOf } from "../../lib/agents";
import { toggleShell } from "../../panels/beside";
import { usePanelHandlers } from "../../panels/usePanelKeys";
import * as byHand from "../../session/byHand";
import { keysOf, type MenuItem } from "../../session/SessionMenu";
import { SessionPane } from "../../session/SessionPane";
import { CloseSession, useSessionActions } from "../../session/useSessionActions";
import { standsBeside } from "../../state/selectors";
import type { Hint } from "../../state/ui";
import { RowDetail } from "./RowDetail";
import { SessionTree } from "./SessionTree";
import { foldable } from "./tree";
import { useTree } from "./useTree";
import "./sessions.css";

const IN_TERMINAL: readonly Hint[] = [
  { key: "⇧Q", does: "give the keys back" },
  { key: "⌘\\", does: "fold the sidebar" },
];

function Empty({ narrowed }: { narrowed: boolean }) {
  return (
    <div className="mu-empty" role="status">
      <h2 className="mu-empty-title">
        {narrowed ? "No sessions in this project" : "No projects yet"}
      </h2>
      <p className="mu-empty-text">
        {narrowed
          ? "Its worktrees and their sessions show here as they start."
          : "Each project you add shows here, with its worktrees and their sessions."}
      </p>
    </div>
  );
}

export default function AllSessions() {
  const app = useApp();
  const actions = useActions();
  const tree = useTree();
  const { selected } = tree;
  const keysIn = useUi((state) => state.keysIn);
  const fullScreen = useUi((state) => state.fullScreen !== null);
  const narrowed = useUi((state) => state.project !== null);
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
  const canFold = useDaemon((state) => (selected ? foldable(state, selected) : false));
  const label = (action: string) => labelOf(map, action);
  const shellKey = label("shell");
  const fullKey = label("full_screen");
  const changesKey = label("show_changes");
  const renameKey = label("rename");
  const closeKey = label("close");
  const pullKey = label("pull_request_in_browser");
  const checksKey = label("checks");
  const reviewKey = label("review");
  const openKey = label("open_pull_request");

  /** The workspace a line works in: a session's, a worktree's, a project's own checkout. */
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
    if (selected.kind !== "session") return tree.fold(selected.id);
    const stored = app.daemon.store.getState().sessions[selected.id];
    if (!stored) return;
    actions.showPane("terminal");
    actions.showTerminal(stored.id);
    actions.keysTo("terminal");
    actions.tell(`Typing in ${stored.title}. Every key goes to ${agentOf(stored.harness).name}.`);
  };
  const out = () => {
    if (!selected) return;
    const folded = tree.folded.has(selected.id);
    if (selected.kind !== "session" && !folded && canFold) return tree.fold(selected.id);
    const parent =
      selected.kind === "session"
        ? selected.workspace
        : selected.kind === "workspace"
          ? selected.project
          : null;
    if (parent) tree.pick(parent);
  };
  const into = () => {
    if (!selected || selected.kind === "session") return;
    if (tree.folded.has(selected.id)) return tree.fold(selected.id);
    const next = tree.rows[at + 1];
    if (next && next.kind !== "project" && next.project === selected.project) tree.pick(next.id);
  };
  const shellHere = () => {
    const workspace = workspaceOf();
    if (!workspace)
      return actions.tell(
        "This project has no checkout to open a shell in; select a worktree.",
        true,
      );
    void toggleShell(app, workspace, null).then((shell) => {
      if (shell) tree.pick(shell);
    });
  };

  // The keys every list has, and the tree's own, as the person set them.
  useKeys({
    ArrowDown: () => move(1),
    ArrowUp: () => move(-1),
    ArrowLeft: out,
    ArrowRight: into,
    Enter: open,
  });
  const panelsHere = usePanelHandlers({ session: null, workspace: workspaceOf() });
  useActionKeys("sessions", {
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
          // The pane's own keys answer on a session; on a project or a worktree, the tree's.
          ...panelsHere,
          shell: shellHere,
          full_screen: () => actions.tell("Select a session to open it full screen.", true),
        }),
  });

  const kind = selected?.kind ?? null;
  // The changes, while shown, say their own keys.
  const changesShown = useUi((state) => state.pane === "changes") && session !== undefined;
  useEffect(() => {
    if (changesShown) return;
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
      return actions.setHints(
        [
          { key: "⏎", does: "fold or unfold" },
          { key: shellKey, does: "shell" },
          { key: checksKey, does: "check" },
          { key: reviewKey, does: "review" },
          { key: openKey, does: "draft pull request" },
          ...(hasPullRequest ? [{ key: pullKey, does: "pull request" }] : []),
        ].filter((hint) => hint.key !== "none"),
      );
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
    checksKey,
    reviewKey,
    openKey,
    changesShown,
  ]);
  useEffect(() => () => actions.setHints([]), [actions]);

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
      <section className="sessions-column" aria-label="All sessions">
        <div className="mu-queue-head">
          <h1 className="mu-queue-title">All sessions</h1>
          <span className="mu-meta">Every project, its worktrees, and their sessions.</span>
        </div>
        {!loaded ? (
          <div className="mu-empty" role="status">
            <p className="mu-empty-text">
              {link === "down" ? "Muster's daemon is out of reach." : "Connecting to Muster…"}
            </p>
          </div>
        ) : tree.rows.length === 0 ? (
          <Empty narrowed={narrowed} />
        ) : (
          <SessionTree
            rows={tree.rows}
            folded={tree.folded}
            selected={selected?.id}
            resting={keysIn !== "list"}
            renaming={tools.renaming ? tools : null}
            panelKeys={panelKeys}
            onPick={(id) => {
              tree.pick(id);
              actions.keysTo("list");
            }}
            onFold={tree.fold}
          />
        )}
      </section>
      {session ? (
        <SessionPane session={session} menu={menu} />
      ) : selected ? (
        <RowDetail row={selected} shellKey={shellKey} />
      ) : (
        <section className="mu-pane app-pane sessions-detail" aria-label="No session">
          <p className="mu-meta sessions-detail-empty">The session you select shows here, live.</p>
        </section>
      )}
      <CloseSession actions={tools} />
    </>
  );
}
