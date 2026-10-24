// The stages as tools on the selected worktree, as the terminal UI's keys
// run them (crates/muster-tui/src/app/events.rs): `c` the project's checks
// and `v` a review by Claude Code, each a session of its own opened with
// the keys as it starts; `o` the branch pushed and a draft pull request
// opened, said in the status bar; `b` the branch's pull request in the
// person's browser. Where none can be, the TUI's words say why.

import type { App } from "../app/context";
import { handsOn, here, START_SIZE } from "../flows/actions";
import { labelOf } from "../flows/keymap";
import { replyAs } from "../ipc/ask";
import type { Command } from "../ipc/generated/Command";
import { errorText } from "../lib/text";
import { whenKnown } from "../state/whenKnown";

function complain(app: App, text: string): void {
  app.actions.tell(text, true);
}

/** Starts the session `command` asks for and opens it, the keys with it (`Then::OpenSession`). */
function startAndOpen(app: App, command: Command): void {
  app.bridge.command(command).then(
    (reply) => {
      const session = replyAs(reply, "SessionStarted")?.id;
      if (session === undefined)
        return complain(app, "The daemon answered something else; try again.");
      app.actions.clearNote();
      whenKnown(app, "sessions", session, () => {
        app.actions.show("all");
        app.actions.select("all", session);
        app.actions.showPane("terminal");
        app.actions.showTerminal(session);
        app.actions.keysTo("terminal");
      });
    },
    (error: unknown) => complain(app, errorText(error)),
  );
}

/** `c`: the project's checks in the selected worktree, opened as they run. */
export function runChecks(app: App): void {
  const workspace = handsOn(app, "run the checks there");
  if (workspace)
    startAndOpen(app, { CheckWorkspace: { workspace: workspace.id, size: START_SIZE } });
}

/** `v`: Claude Code reviews the selected worktree's change, opened as it starts. */
export function review(app: App): void {
  const workspace = handsOn(app, "review its change");
  if (workspace)
    startAndOpen(app, { ReviewWorkspace: { workspace: workspace.id, size: START_SIZE } });
}

/** `o`: pushes the selected worktree's branch and opens a draft pull request from it. */
export function openPullRequest(app: App): void {
  const workspace = handsOn(app, "open a pull request from it");
  if (!workspace) return;
  app.actions.tell("Pushing the branch and opening a draft pull request…");
  app.bridge.command({ OpenPullRequest: { workspace: workspace.id } }).then(
    (reply) => {
      const opened = replyAs(reply, "PullRequestOpened");
      if (!opened) return complain(app, "The daemon answered something else; try again.");
      const { pr } = opened;
      app.actions.tell(`Draft pull request #${pr.number} is open: ${pr.url}`);
    },
    (error: unknown) => complain(app, errorText(error)),
  );
}

/** `b`: the pull request from the selected worktree's branch, in the browser. */
export function browsePullRequest(app: App): void {
  const pr = here(app).workspace?.pull_request;
  if (!pr) {
    const open = labelOf(app.daemon.store.getState(), "open_pull_request");
    complain(app, `No pull request from this branch is known; ${open} opens one.`);
    return;
  }
  app.bridge.openUrl(pr.url).catch((error: unknown) => complain(app, errorText(error)));
}
