// *All sessions* (the AllSessions board; design system: SessionTree): every
// project, its worktrees and their sessions, as the model lists them, and
// beside the tree the selected session's pane, live, with its shell and
// plugins' panels. The keys are the terminal UI's: ↑↓ (k j) move, ← → fold
// and unfold (or go out and in), `e` folds, ⏎ types in the session or folds
// the line, `T` a shell, `f` full screen, a plugin's key its panel, `d` the
// changes, `R` renames, `x` closes, `b` the pull request.

import { useEffect } from "react";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { type KeyHandlers, useKeys } from "../../app/keys";
import { agentOf } from "../../lib/agents";
import { togglePanel, toggleShell } from "../../panels/beside";
import { bind, usePanelActions, writtenKeys } from "../../panels/usePanelKeys";
import type { MenuItem } from "../../session/SessionMenu";
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
  const away = useDaemon((state) => (state.link.state === "down" ? state.link.reason : null));
  const keymap = useDaemon((state) => state.preferences?.keys);
  const panelActions = usePanelActions();
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
  const shellKey = writtenKeys(keymap, "shell", ["T"])[0] ?? "T";
  const fullKey = writtenKeys(keymap, "full_screen", ["f"])[0] ?? "f";

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

  const keys: Record<string, () => void> = {
    ArrowDown: () => move(1),
    ArrowUp: () => move(-1),
    j: () => move(1),
    k: () => move(-1),
    ArrowLeft: out,
    ArrowRight: into,
    Enter: open,
  };
  bind(keys, writtenKeys(keymap, "fold", ["e"]), () => selected && tree.fold(selected.id));
  if (session) {
    bind(keys, writtenKeys(keymap, "show_changes", ["d"]), () => actions.showPane("changes"));
    bind(keys, writtenKeys(keymap, "rename", ["R"]), tools.rename);
    bind(keys, writtenKeys(keymap, "close", ["x"]), tools.close);
    bind(keys, writtenKeys(keymap, "pull_request_in_browser", ["b"]), tools.browser);
  } else {
    // The pane's own keys answer on a session; on a project or a worktree, the tree's.
    bind(keys, writtenKeys(keymap, "shell", ["T"]), shellHere);
    bind(keys, writtenKeys(keymap, "full_screen", ["f"]), () =>
      actions.tell("Select a session to open it full screen.", true),
    );
    for (const action of panelActions)
      bind(
        keys,
        action.keys,
        () => void togglePanel(app, action, { session: null, workspace: workspaceOf() }),
      );
  }
  useKeys(keys as KeyHandlers);

  const kind = selected?.kind ?? null;
  useEffect(() => {
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
        { key: "d", does: "see changes" },
        ...(isChat ? [{ key: "R", does: "rename" }] : []),
        { key: "x", does: "close" },
        ...(hasPullRequest ? [{ key: "b", does: "pull request" }] : []),
      ]);
    if (kind)
      return actions.setHints([
        { key: "⏎", does: "fold or unfold" },
        { key: shellKey, does: "shell" },
      ]);
    actions.setHints([]);
  }, [actions, keysIn, fullScreen, kind, isChat, hasPullRequest, shellKey, fullKey]);
  useEffect(() => () => actions.setHints([]), [actions]);

  const panelKeys: Record<string, string> = {};
  for (const action of panelActions) {
    const key = action.keys[0];
    if (key) panelKeys[`plugin:${action.plugin}/${action.panel}`] = key;
  }
  const menu: MenuItem[] = [
    ...(isChat ? [{ key: "R", label: "Rename", does: tools.rename }] : []),
    ...(hasPullRequest
      ? [{ key: "b", label: "Pull request in the browser", does: tools.browser }]
      : []),
    { key: "x", label: "Close", does: tools.close, danger: true },
  ];

  return (
    <>
      <section className="sessions-column" aria-label="All sessions">
        <div className="mu-queue-head">
          <h1 className="mu-queue-title">All sessions</h1>
          <span className="mu-meta">Every project, its worktrees, and their sessions.</span>
        </div>
        {away ? (
          <div className="mu-banner mu-banner--away sessions-away" role="alert">
            <span className="mu-banner-text">
              <span className="mu-banner-lead">Out of reach:</span> {away}
            </span>
          </div>
        ) : null}
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
