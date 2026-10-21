// The commands the mock daemon answers. Each page adds the ones it sends,
// answering as the real daemon would (its replies are typed by the
// generated `Reply`). An unknown command is refused, so a test notices.

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { MockBridge } from "./bridge";

export function handleCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (command === "Ping") return "Pong";
  if (typeof command === "string") return undefined;

  if (
    "WorkspaceChanges" in command ||
    "FileDiff" in command ||
    "Timeline" in command ||
    "EntryDiff" in command
  ) {
    const request =
      "WorkspaceChanges" in command
        ? command.WorkspaceChanges
        : "FileDiff" in command
          ? command.FileDiff
          : "Timeline" in command
            ? command.Timeline
            : command.EntryDiff;
    const workspace = mock.world.workspaces.find((found) => found.id === request.workspace);
    if (!workspace)
      throw new BridgeFailure("refused", `no workspace ${request.workspace}`, "not_found");
    if (!workspace.kind.branch)
      throw new BridgeFailure(
        "refused",
        "that is a plain folder, not a git repository: git cannot tell what changed there",
        "bad_request",
      );
    if (workspace.kind.kind === "container")
      throw new BridgeFailure(
        "refused",
        "a run's container keeps its own clone; its changes are the run's",
        "bad_request",
      );
    if ("WorkspaceChanges" in command)
      return {
        Changes: mock.world.changes[workspace.id] ?? {
          base: "main",
          uncommitted: [],
          branch: [],
          commits: [],
          more_commits: 0,
        },
      };
    if ("Timeline" in command)
      return { Timeline: mock.world.timelines[workspace.id] ?? { entries: [] } };
    if ("FileDiff" in command) {
      const { part, path } = command.FileDiff;
      if (!mock.world.changes[workspace.id]?.[part].some((file) => file.path === path))
        throw new BridgeFailure(
          "refused",
          `${path} is not among the files changed there`,
          "not_found",
        );
      return {
        FileDiff: mock.world.diffs[`${workspace.id}:${part}:${path}`] ?? { text: "", cut: false },
      };
    }
    if ("EntryDiff" in command) {
      const { number } = command.EntryDiff;
      if (!mock.world.timelines[workspace.id]?.entries.some((entry) => entry.number === number))
        throw new BridgeFailure(
          "refused",
          `no entry ${number} is kept in this workspace's timeline`,
          "not_found",
        );
      return {
        FileDiff: mock.world.diffs[`${workspace.id}:entry:${number}`] ?? { text: "", cut: false },
      };
    }
  }

  if ("RenameSession" in command) {
    const { session, title } = command.RenameSession;
    mock.updateSession(session, { title });
    mock.emitSession(session);
    return "Done";
  }
  if ("StopSession" in command) {
    const { session } = command.StopSession;
    mock.setStatus(session, { state: "exited", success: true });
    return "Done";
  }
  if ("RemoveSession" in command) {
    mock.removeSession(command.RemoveSession.session);
    return "Done";
  }
  return undefined;
}
