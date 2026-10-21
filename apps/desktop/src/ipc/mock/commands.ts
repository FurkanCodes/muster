// The commands the mock daemon answers. Each page adds the ones it sends,
// answering as the real daemon would (its replies are typed by the
// generated `Reply`). An unknown command is refused, so a test notices.

import { BridgeFailure } from "../bridge";
import type { ChangeSummary } from "../generated/ChangeSummary";
import type { Changes } from "../generated/Changes";
import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { MockBridge } from "./bridge";
import { flowCommand } from "./flows";
import { NOW } from "./now";
import { sessionsCommand } from "./sessions";
import { ticketCommand } from "./tickets";
import { handleUndo } from "./undo";

export function handleCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (command === "Ping") return "Pong";
  const sessions = sessionsCommand(mock, command);
  if (sessions !== undefined) return sessions;
  const settings = mock.settings.handle(command);
  if (settings !== undefined) return settings;
  if (typeof command === "string") return undefined;
  const undone = handleUndo(mock, command);
  if (undone !== undefined) return undone;

  if ("RenameSession" in command) {
    const { session, title } = command.RenameSession;
    if (!mock.world.sessions.some((stored) => stored.id === session))
      throw new BridgeFailure("refused", `no session ses_${session}`, "not_found");
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
    if (!stored) throw new BridgeFailure("refused", `ses_${session} is not running`, "not_found");
    if (stored.status.state === "exited")
      throw new BridgeFailure("refused", `ses_${session} has already exited`, "conflict");
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
    if (!stored) throw new BridgeFailure("refused", `no session ses_${id}`, "not_found");
    if (stored.status.state !== "exited")
      throw new BridgeFailure(
        "refused",
        `${stored.title} is still running; stop it first`,
        "conflict",
      );
    mock.removeSession(id);
    return "Done";
  }
  const changes = changesCommand(mock, command);
  if (changes !== undefined) return changes;
  return ticketCommand(mock, command) ?? flowCommand(mock, command);
}

/** `Changes::summary`. */
function summaryOf(changes: Changes): ChangeSummary | null {
  const files = [...changes.uncommitted, ...changes.branch];
  if (files.length === 0) return null;
  return {
    files: new Set(files.map((file) => file.path)).size,
    added: files.reduce((sum, file) => sum + (file.lines?.added ?? 0), 0),
    removed: files.reduce((sum, file) => sum + (file.lines?.removed ?? 0), 0),
  };
}

/**
 * A workspace's changes and timeline (supervisor::changes,
 * supervisor::timeline): `WorkspaceChanges`, `FileDiff`, `Timeline` and
 * `EntryDiff`, refused where git cannot tell what changed.
 */
function changesCommand(mock: MockBridge, command: Exclude<Command, string>): Reply | undefined {
  const request =
    "WorkspaceChanges" in command
      ? command.WorkspaceChanges
      : "FileDiff" in command
        ? command.FileDiff
        : "Timeline" in command
          ? command.Timeline
          : "EntryDiff" in command
            ? command.EntryDiff
            : null;
  if (!request) return undefined;
  const id = request.workspace;
  const workspace = mock.world.workspaces.find((stored) => stored.id === id);
  if (!workspace) throw new BridgeFailure("refused", `no workspace wsp_${id}`, "not_found");
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
  if ("WorkspaceChanges" in command) {
    const changes = mock.world.changes?.[id] ?? {
      base: "main",
      uncommitted: [],
      branch: [],
      commits: [],
      more_commits: 0,
    };
    // The workspace's summary is kept in step with what a client is told.
    const summary = summaryOf(changes);
    if (JSON.stringify(summary) !== JSON.stringify(workspace.changes))
      queueMicrotask(() => mock.updateWorkspace(id, { changes: summary }));
    return { Changes: changes };
  }
  if ("FileDiff" in command) {
    const { part, path } = command.FileDiff;
    if (!mock.world.changes?.[id]?.[part].some((file) => file.path === path))
      throw new BridgeFailure("refused", `${path} is no longer changed there`, "not_found");
    return { FileDiff: mock.world.diffs?.[`${id}:${part}:${path}`] ?? { text: "", cut: false } };
  }
  if ("Timeline" in command) return { Timeline: mock.world.timelines?.[id] ?? { entries: [] } };
  if (!("EntryDiff" in command)) return undefined;
  const { number } = command.EntryDiff;
  if (!mock.world.timelines?.[id]?.entries.some((entry) => entry.number === number))
    throw new BridgeFailure(
      "refused",
      `there is no entry ${number} in this workspace's timeline`,
      "not_found",
    );
  return { FileDiff: mock.world.entryDiffs?.[id]?.[number] ?? { text: "", cut: false } };
}
