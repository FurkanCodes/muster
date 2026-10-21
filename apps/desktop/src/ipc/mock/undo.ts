// supervisor::timeline and timeline::undo, over snapshots of the mock world.
import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { MockBridge } from "./bridge";
import { NOW } from "./now";

export function handleUndo(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string" || !("UndoPlan" in command || "Undo" in command)) return;
  const request = "UndoPlan" in command ? command.UndoPlan : command.Undo;
  const { workspace, number } = request;
  const place = mock.world.workspaces.find((found) => found.id === workspace);
  if (!place) throw new BridgeFailure("refused", `no workspace ${workspace}`, "not_found");
  if (!place.kind.branch)
    throw new BridgeFailure(
      "refused",
      "that is a plain folder, not a git repository: git cannot tell what changed there",
      "bad_request",
    );
  if (place.kind.kind === "container")
    throw new BridgeFailure(
      "refused",
      "a run's container keeps its own clone; its changes are the run's",
      "bad_request",
    );
  const timeline = mock.world.timelines?.[workspace];
  const entry = timeline?.entries.find((entry) => entry.number === number);
  if (!timeline || !entry)
    throw new BridgeFailure(
      "refused",
      `there is no entry ${number} in this workspace's timeline`,
      "not_found",
    );
  const key = `${workspace}:${number}`;
  const plan = mock.world.undoPlans?.[key] ?? {
    number,
    later: timeline.entries.filter((other) => other.number > number && other.summary).length,
    branch: null,
  };
  if ("UndoPlan" in command) return { UndoPlan: structuredClone(plan) };
  const busy = mock.world.sessions.find(
    (session) =>
      session.workspace === workspace &&
      (session.status.state === "running" || session.status.state === "needs_you"),
  );
  if (busy)
    throw new BridgeFailure(
      "refused",
      `${busy.title} is at work there: put the files back once it has stopped`,
      "conflict",
    );
  if (command.Undo.move_branch && plan.branch?.refused)
    throw new BridgeFailure(
      "refused",
      `the branch stays where it is: ${plan.branch.refused}`,
      "conflict",
    );
  mock.world.changes ??= {};
  mock.world.beforeUndo ??= {};
  const changes = mock.world.changes;
  const kept = mock.world.beforeUndo;
  const current = changes[workspace];
  const before = kept[key];
  if (current && before && JSON.stringify(current) === JSON.stringify(before))
    throw new BridgeFailure(
      "refused",
      `the files are already as they were before entry ${number}`,
      "conflict",
    );
  const recorded = Math.max(...timeline.entries.map((entry) => entry.number), 0) + 1;
  if (current) kept[`${workspace}:${recorded}`] = structuredClone(current);
  if (before) {
    const restored = structuredClone(before);
    if (!command.Undo.move_branch && current) {
      restored.commits = current.commits;
      restored.branch = current.branch;
      restored.more_commits = current.more_commits;
    }
    changes[workspace] = restored;
  }
  timeline.entries.unshift({
    number: recorded,
    kind: { kind: "undo", of: number },
    started: NOW,
    ended: NOW,
    summary: entry.summary,
    head_before: entry.head_after,
    head_after: entry.head_before,
  });
  timeline.entries = timeline.entries.slice(0, 100);
  mock.world.entryDiffs ??= {};
  mock.world.entryDiffs[workspace] ??= {};
  const diffs = mock.world.entryDiffs[workspace];
  diffs[recorded] = diffs[number] ?? { text: "", cut: false };
  mock.updateWorkspace(workspace, { timeline: recorded });
  return {
    Undone: { entry: recorded, moved_branch: command.Undo.move_branch && plan.branch !== null },
  };
}
