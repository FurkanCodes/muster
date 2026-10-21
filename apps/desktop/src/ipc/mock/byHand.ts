// The stages by hand, answered as the daemon answers them, with its
// refusals word for word (crates/muster-daemon/src/supervisor/by_hand.rs):
// the project's checks and a review each start a session of their own; a
// draft pull request is "pushed" and opened, or the one open is found.

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { Workspace } from "../generated/Workspace";
import type { MockBridge } from "./bridge";
import { NOW, ulid } from "./scenario";

/** What the projects' settings and the host say, which tests change. */
export class ByHandWorld {
  /** The projects whose settings name no checks. */
  readonly noChecks = new Set<string>();
  /** The number the next pull request opened gets. */
  nextPullRequest = 120;
  next = 0;
}

function refuse(code: "bad_request" | "not_found", message: string): never {
  throw new BridgeFailure("refused", message, code);
}

/** `reviewable_branch`: the branch checked out, where it can go anywhere. */
function branchOf(workspace: Workspace): string {
  if (!workspace.kind.branch)
    refuse(
      "bad_request",
      "that is a plain folder, not a git repository: it has no branch to review or push",
    );
  return workspace.kind.branch;
}

export function byHandCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string") return undefined;
  const request =
    "CheckWorkspace" in command
      ? command.CheckWorkspace
      : "ReviewWorkspace" in command
        ? command.ReviewWorkspace
        : "OpenPullRequest" in command
          ? command.OpenPullRequest
          : null;
  if (!request) return undefined;
  const workspace =
    mock.world.workspaces.find((found) => found.id === request.workspace) ??
    refuse("not_found", `no workspace ${request.workspace}`);
  const container = workspace.kind.kind === "container";

  const start = (harness: "checks" | "review", title: string): Reply => {
    const started: Session = {
      id: ulid(7000 + ++mock.byHand.next),
      workspace: workspace.id,
      harness,
      title,
      status: { state: "running" },
      unseen: false,
      resume_id: null,
      created_at: NOW,
      ended_at: null,
      status_since: NOW,
      activity: null,
      spend: null,
      tidy: null,
      beside: null,
    };
    mock.world.sessions.push(started);
    // As the daemon does, the reply comes first and the session after it.
    setTimeout(() => mock.publish({ sessions: [started] }), 0);
    return { SessionStarted: { id: started.id } };
  };

  if ("CheckWorkspace" in command) {
    if (container)
      refuse("bad_request", "checks by hand run in a worktree or the project's checkout");
    if (mock.byHand.noChecks.has(workspace.project))
      refuse(
        "bad_request",
        'the project names no checks: add `[checks] commands = ["…"]` to muster.toml',
      );
    return start("checks", "Checks");
  }
  if ("ReviewWorkspace" in command) {
    if (container)
      refuse("bad_request", "a review by hand runs in a worktree or the project's checkout");
    branchOf(workspace);
    return start("review", "Review");
  }

  if (container)
    refuse("bad_request", "a run opens its own pull request; hand it back to have it opened");
  const branch = branchOf(workspace);
  if (branch === "main")
    refuse("bad_request", `${branch} is the branch a pull request would merge into`);
  // One open already is found, not opened again.
  const open = workspace.pull_request;
  if (open) return { PullRequestOpened: { pr: { number: open.number, url: open.url } } };
  const number = mock.byHand.nextPullRequest++;
  const name = mock.world.projects.find((found) => found.id === workspace.project)?.name ?? "app";
  const url = `https://github.com/o/${name}/pull/${number}`;
  mock.updateWorkspace(workspace.id, {
    pull_request: { number, url, state: "draft", checks: null, review: null },
  });
  return { PullRequestOpened: { pr: { number, url } } };
}
