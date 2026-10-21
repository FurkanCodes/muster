// The daemon answers a command that makes something (a project, a worktree,
// a session) before the change it made reaches the window, so a list told
// to select it at once has not drawn it yet. What is done with it waits for
// it in the daemon's state, as the terminal UI's `wanted` does.

import type { App } from "../app/context";

type Kind = "projects" | "workspaces" | "sessions";

/** How many changes of the state it waits through before giving up. */
const PATIENCE = 200;

/**
 * Runs `then` once the daemon's state holds `id` among its `kind`: at once
 * if it already does. Gives up, without timers, after `PATIENCE` changes
 * of the state that never brought it. Returns a way to stop waiting.
 */
export function whenKnown(app: App, kind: Kind, id: string, then: () => void): () => void {
  const store = app.daemon.store;
  if (id in store.getState()[kind]) {
    then();
    return () => {};
  }
  let changes = 0;
  const stop = store.subscribe((state) => {
    changes += 1;
    if (id in state[kind]) {
      stop();
      then();
    } else if (changes >= PATIENCE) stop();
  });
  return stop;
}
