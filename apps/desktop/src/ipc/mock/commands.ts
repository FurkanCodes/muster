// The commands the mock daemon answers. Each page adds the ones it sends,
// answering as the real daemon would (its replies are typed by the
// generated `Reply`). An unknown command is refused, so a test notices.

import type { Command } from "../generated/Command";
import type { Reply } from "../generated/Reply";
import type { MockBridge } from "./bridge";

export function handleCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (command === "Ping") return "Pong";
  if (typeof command === "string") return undefined;

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
