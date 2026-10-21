// The questions about a worktree's branch, as the TUI asks them
// (crates/muster-tui/src/app/tidy.rs, stacks.rs) and the boards draw them
// (design/screens/Dialogs.html): tidying its commits (t), the agent asked
// first, ⇥ ⇧⇥ going round the agents; and moving it onto the newest of
// what it is stacked on, or of its base (u). The daemon refuses what it
// cannot do and says why.

import type { KeyboardEvent } from "react";
import { useState } from "react";
import { useApp, useDaemon, useUi } from "../app/context";
import type { AddedAgent } from "../ipc/generated/AddedAgent";
import type { WorkspaceId } from "../ipc/generated/WorkspaceId";
import { whenKnown } from "../state/whenKnown";
import { AgentChoice } from "../ui/AgentChoice";
import { Ask } from "./Ask";
import { START_SIZE } from "./actions";
import { offered, stepAgent } from "./agents";
import { count, useChanges } from "./changes";
import { labelOf } from "./keymap";
import { useTask } from "./useTask";

const NO_AGENTS: AddedAgent[] = [];

/** The commits since the base, as git said them, to say before acting. */
function commitsSince(changes: ReturnType<typeof useChanges>): string | null {
  if (!changes) return null;
  const commits = changes.commits.length + changes.more_commits;
  return `${count(commits, "commit")}${changes.base ? ` since ${changes.base}` : ""}`;
}

export function Tidy({ workspace }: { workspace: WorkspaceId }) {
  const app = useApp();
  const branch = useDaemon((state) => state.workspaces[workspace]?.kind.branch ?? "");
  const added = useDaemon((state) => state.preferences?.agents ?? NO_AGENTS);
  const last = useUi((state) => state.lastAgent);
  const agents = offered(added, last, "tidy");
  const [harness, setHarness] = useState(() => agents[0]?.id ?? "claude");
  const name = agents.find((agent) => agent.id === harness)?.name ?? harness;
  const since = commitsSince(useChanges(workspace));
  const task = useTask();
  const tidy = () =>
    void task.run(async () => {
      const reply = await app.bridge.command({
        TidyCommits: { workspace, harness, size: START_SIZE },
      });
      if (typeof reply !== "object" || !("SessionStarted" in reply))
        throw new Error("The daemon answered something else; try again.");
      const session = reply.SessionStarted.id;
      app.actions.choseAgent(harness);
      app.actions.openFlow(null);
      app.actions.show("all");
      whenKnown(app, "sessions", session, () => {
        app.actions.select("all", session);
        app.actions.showTerminal(session);
      });
      app.actions.tell("Starting the tidy…");
    });
  // ⇥ ⇧⇥ (and the arrows) go round the agents, as in the TUI's dialog.
  const onKeyDown = (event: KeyboardEvent<HTMLDialogElement>) => {
    const by =
      event.key === "Tab"
        ? event.shiftKey
          ? -1
          : 1
        : ({ ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 } as const)[event.key];
    if (
      by === undefined ||
      (event.key !== "Tab" &&
        !(event.target instanceof HTMLElement && event.target.closest("[role=radiogroup]")))
    )
      return;
    event.preventDefault();
    const next = stepAgent(agents, harness, by as 1 | -1);
    setHarness(next);
    event.currentTarget
      .querySelectorAll<HTMLElement>("[role=radio]")
      [agents.findIndex((agent) => agent.id === next)]?.focus();
  };
  return (
    <Ask
      title={`Tidy the commits on ${branch}?`}
      meta={
        <>
          {since ? `${since}. ` : null}An agent rewrites those not pushed yet into clear ones, the
          files left as they are; its turn goes in the timeline, so it can be put back.
        </>
      }
      act={`Tidy with ${name}`}
      acting="Starting the tidy…"
      keep="Cancel"
      task={task}
      onAct={tidy}
      onKeyDown={onKeyDown}
    >
      <AgentChoice agents={agents} value={harness} last={last} onChange={setHarness} />
    </Ask>
  );
}

export function Move({ workspace }: { workspace: WorkspaceId }) {
  const app = useApp();
  const found = useDaemon((state) => state.workspaces[workspace]);
  // What it is stacked on, and whether that has merged, as its pull request
  // last said: "gone" once no worktree holds it, else "state number".
  const stacked = useDaemon((state) => state.workspaces[workspace]?.stacked_on?.branch ?? null);
  const pull = useDaemon((state) => {
    const holder = Object.values(state.workspaces).find(
      (other) => other.project === found?.project && other.kind.branch === stacked,
    );
    return holder
      ? `${holder.pull_request?.state ?? ""} ${holder.pull_request?.number ?? ""}`
      : "gone";
  });
  const [pullState, pullNumber] = pull.split(" ");
  const parent = stacked
    ? {
        branch: stacked,
        merged: pull === "gone" || pullState === "merged",
        number: pullState === "merged" && pullNumber ? pullNumber : null,
      }
    : null;
  const changes = useChanges(workspace);
  const task = useTask();
  const branch = found?.kind.branch ?? "the branch";
  const base = changes?.base ?? "its base";
  const onto = parent && !parent.merged ? parent.branch : base;
  const commits = changes
    ? count(changes.commits.length + changes.more_commits, "commit")
    : "commits";
  const move = () =>
    void task.run(async () => {
      const reply = await app.bridge.command({ MoveOnto: { workspace } });
      if (typeof reply !== "object" || !("Moved" in reply))
        throw new Error("The daemon answered something else; try again.");
      const { onto: moved, commits: replayed, entry } = reply.Moved;
      const state = app.daemon.store.getState();
      const keys = (action: "put_back" | "show_changes") => labelOf(state.preferences, action);
      const open = state.workspaces[workspace]?.pull_request;
      const push =
        open && (open.state === "open" || open.state === "draft")
          ? " Its pull request follows once it is pushed with --force-with-lease."
          : "";
      app.actions.openFlow(null);
      app.actions.tell(
        `${branch} is on ${moved} now, its ${count(replayed, "commit")} replayed there; ${keys("put_back")} on #${entry} in ${keys("show_changes")} puts it back.${push}`,
      );
    });
  return (
    <Ask
      title={`Move ${branch} onto ${onto}?`}
      act="Move it"
      acting="Moving…"
      task={task}
      onAct={move}
    >
      <p className="mu-dialog-text">
        {parent?.merged
          ? `The branch it was stacked on, ${parent.branch}, merged${parent.number ? ` as #${parent.number}` : ""}. `
          : parent
            ? `It is stacked on ${parent.branch}. `
            : null}
        Its {commits} are replayed on {onto}'s newest; if one would conflict, nothing changes. What
        is there is kept first, so the move can be put back.
      </p>
    </Ask>
  );
}
