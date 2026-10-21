// The commands the mock daemon answers. Each page adds the ones it sends,
// answering as the real daemon would (its replies are typed by the
// generated `Reply`). An unknown command is refused, so a test notices.

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { MockBridge } from "./bridge";
import { NOW } from "./now";
import { sessionsCommand } from "./sessions";
import { ticketCommand } from "./tickets";

export function handleCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (command === "Ping") return "Pong";
  if (typeof command === "string") return undefined;

  if ("RenameSession" in command) {
    const { session, title } = command.RenameSession;
    if (!mock.world.sessions.some((stored) => stored.id === session))
      throw new BridgeFailure("refused", `no session ${session}`, "not_found");
    const name = Array.from(title.replace(/\s+/g, " ").trim()).slice(0, 80).join("").trimEnd();
    if (!name)
      throw new BridgeFailure("refused", "a session's name cannot be empty", "bad_request");
    mock.updateSession(session, { title: name });
    mock.emitSession(session);
    return "Done";
  }
  if ("StopSession" in command) {
    const { session } = command.StopSession;
    const stored = mock.world.sessions.find((stored) => stored.id === session);
    if (!stored) throw new BridgeFailure("refused", `${session} is not running`, "not_found");
    if (stored.status.state === "exited")
      throw new BridgeFailure("refused", `${session} has already exited`, "conflict");
    // The daemon answers at once and tells the program's exit when it comes.
    queueMicrotask(() => {
      mock.updateSession(session, { ended_at: NOW, activity: null });
      mock.setStatus(session, { state: "exited", success: true });
    });
    return "Done";
  }
  if ("RemoveSession" in command) {
    const id = command.RemoveSession.session;
    const stored = mock.world.sessions.find((stored) => stored.id === id);
    if (!stored) throw new BridgeFailure("refused", `no session ${id}`, "not_found");
    if (stored.status.state !== "exited")
      throw new BridgeFailure(
        "refused",
        `${stored.title} is still running; stop it first`,
        "conflict",
      );
    mock.removeSession(id);
    return "Done";
  }
  if ("Timeline" in command || "EntryDiff" in command) {
    const id = "Timeline" in command ? command.Timeline.workspace : command.EntryDiff.workspace;
    const workspace = mock.world.workspaces.find((stored) => stored.id === id);
    if (!workspace) throw new BridgeFailure("refused", `no workspace ${id}`, "not_found");
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
    if ("Timeline" in command) return { Timeline: mock.world.timelines?.[id] ?? { entries: [] } };
    const number = command.EntryDiff.number;
    const diff = mock.world.entryDiffs?.[id]?.[number];
    if (!diff)
      throw new BridgeFailure(
        "refused",
        `there is no entry ${number} in this workspace's timeline`,
        "not_found",
      );
    return { FileDiff: diff };
  }
  return sessionsCommand(mock, command) ?? ticketCommand(mock, command);
}
