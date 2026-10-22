// The flows over the window: their keys on the screens they work on (as
// `muster_core::keys` places them), a folder dropped on the window added as
// a project, and the palette or the dialog open, each loaded when first
// asked (`ARC-25`). Nothing here asks the daemon while it is out of reach.

import { lazy, Suspense, useEffect } from "react";
import { type App, useApp, useDaemon, useUi } from "../app/context";
import { type KeyHandlers, useKeys } from "../app/keys";
import type { ScreenId } from "../state/ui";
import * as act from "./actions";
import { type KeyAction, keysOf } from "./keymap";
// The palette comes with the window: what is typed straight after ⌘K must
// reach its field. The dialogs load when first asked.
import Palette from "./Palette";

const NewWorktree = lazy(() => import("./NewWorktree"));
const CloneProject = lazy(() => import("./CloneProject"));
const AddProject = lazy(() => import("./AddProject"));
const Questions = lazy(() => import("./Questions"));

/** The flows' keys, and the screens each works on. */
const BOUND: readonly [KeyAction, (app: App) => void, readonly ScreenId[]][] = [
  ["add_project", act.addProject, act.SESSION_LISTS],
  ["clone_project", act.cloneProject, act.SESSION_LISTS],
  ["new_session", act.newSession, act.SESSION_LISTS],
  ["new_worktree", act.newWorktree, act.SESSION_LISTS],
  ["close", act.close, act.SESSION_LISTS],
  ["stop_session", act.stop, act.SESSION_LISTS],
  ["restart_session", act.restart, act.SESSION_LISTS],
  ["tidy", act.tidy, ["all"]],
  ["move_branch", act.moveBranch, ["all"]],
];

function useFlowKeys(app: App): void {
  const screen = useUi((state) => state.screen);
  const preferences = useDaemon((state) => state.preferences);
  const linked = useDaemon((state) => state.link.state === "up");
  const handlers: Record<string, KeyHandlers[string]> = {};
  if (linked) {
    for (const [action, run, screens] of BOUND) {
      if (!screens.includes(screen)) continue;
      for (const key of keysOf(preferences, action)) handlers[key.chord] = () => run(app);
    }
  }
  for (const key of keysOf(preferences, "help"))
    handlers[key.chord] = () => app.actions.openPalette(true, "keys");
  useKeys(handlers, { beneath: true });
}

/** A folder dropped on the window is added as a project, while the person can see it. */
function useFolderDrop(app: App): void {
  useEffect(() => {
    let gone = false;
    let unlisten: (() => void) | undefined;
    const adding = new Set<string>();
    app.bridge
      .onFolderDrop((paths) => {
        if (app.daemon.store.getState().link.state !== "up") return;
        for (const path of paths) {
          if (adding.has(path)) continue;
          adding.add(path);
          act
            .addProjectAt(app, path)
            .catch((error: unknown) => app.actions.tell(act.said(error), true))
            .finally(() => adding.delete(path));
        }
      })
      .then(
        (stop) => {
          if (gone) stop();
          else unlisten = stop;
        },
        () => {},
      );
    return () => {
      gone = true;
      unlisten?.();
    };
  }, [app]);
}

/**
 * In a mock build, tests select what the screens of other branches would
 * (a worktree's line, a project's) through `window.__musterUi`.
 */
function useMockHook(app: App): void {
  useEffect(() => {
    if (!__MOCK__) return;
    const hooks = window as unknown as { __musterUi?: App["actions"] };
    hooks.__musterUi = app.actions;
    return () => {
      delete hooks.__musterUi;
    };
  }, [app]);
}

export function Flows() {
  const app = useApp();
  useFlowKeys(app);
  useFolderDrop(app);
  useMockHook(app);
  const flow = useUi((state) => state.flow);
  const palette = useUi((state) => state.paletteOpen);
  const linked = useDaemon((state) => state.link.state === "up");
  useEffect(() => {
    // What was asked goes with the daemon: nothing would answer it.
    if (linked) return;
    app.actions.openFlow(null);
    app.actions.openPalette(false);
  }, [app, linked]);
  if (!linked) return null;
  return (
    <Suspense fallback={null}>
      {palette ? (
        <Palette />
      ) : flow?.kind === "add-project" ? (
        <AddProject />
      ) : flow?.kind === "clone-project" ? (
        <CloneProject />
      ) : flow?.kind === "new-worktree" ? (
        <NewWorktree flow={flow} />
      ) : flow ? (
        <Questions flow={flow} />
      ) : null}
    </Suspense>
  );
}
