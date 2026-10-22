// The real bridge: the app's Rust side, through Tauri's IPC. State comes on
// one ordered channel; each attached terminal has its own channel of raw
// byte frames.

import { Channel, invoke } from "@tauri-apps/api/core";
import { type Bridge, BridgeFailure } from "./bridge";
import { decodeFrame, type TermFrame } from "./frames";
import type { Command } from "./generated/Command";
import type { BridgeError } from "./generated/desktop/BridgeError";
import type { Folders } from "./generated/desktop/Folders";
import type { PickedFolder } from "./generated/desktop/PickedFolder";
import type { StateMessage } from "./generated/desktop/StateMessage";
import type { Reply } from "./generated/Reply";
import type { SessionId } from "./generated/SessionId";
import type { TerminalColours } from "./generated/TerminalColours";

function failure(error: unknown): BridgeFailure {
  const known = error as Partial<BridgeError> | undefined;
  switch (known?.kind) {
    case "unreachable":
      return new BridgeFailure("unreachable", "Muster's daemon is out of reach");
    case "refused":
      return new BridgeFailure("refused", known.message ?? "refused", known.code);
    case "invalid":
      return new BridgeFailure("invalid", known.message ?? "invalid");
    default:
      return new BridgeFailure("invalid", String(error));
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw failure(error);
  }
}

export class TauriBridge implements Bridge {
  async open(onMessage: (message: StateMessage) => void): Promise<void> {
    const onState = new Channel<StateMessage>();
    onState.onmessage = onMessage;
    await call("link_open", { onState });
  }

  tryNow(): Promise<void> {
    return call("try_now");
  }

  showDaemonLog(): Promise<void> {
    return call("show_daemon_log");
  }

  command(command: Command): Promise<Reply> {
    return call<Reply>("command", { command });
  }

  openUrl(url: string): Promise<void> {
    return call("open_ticket_url", { url });
  }

  /** Plays a sound by its name, as it is chosen (`screens/settings/hear.ts`). */
  hear(sound: string): Promise<void> {
    return call("hear", { sound });
  }

  /** Restarts the daemon in place, as `muster reload` does; what it said (`screens/settings/asks.ts`). */
  restartInPlace(): Promise<string> {
    return call<string>("restart_in_place");
  }

  openNotificationSettings(): Promise<void> {
    return call("open_notification_settings");
  }

  editSettingsFile(): Promise<void> {
    return call("edit_settings_file");
  }

  /** The window's own state the app keeps (`state/kept.ts`). */
  kept(): Promise<Record<string, string>> {
    return call("kept");
  }

  keep(key: string, value: string | null): Promise<void> {
    return call("keep", { key, value });
  }

  async attach(
    session: SessionId,
    fromSeq: number | null,
    watching: boolean,
    onFrame: (frame: TermFrame) => void,
  ): Promise<void> {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (buffer) => onFrame(decodeFrame(buffer));
    await call("attach", { session, fromSeq, watching, onFrame: channel });
  }

  detach(session: SessionId): Promise<void> {
    return call("detach", { session });
  }

  input(session: SessionId, text: string): Promise<void> {
    return call("input", { session, text });
  }

  resize(session: SessionId, cols: number, rows: number, width?: number, height?: number) {
    return call<void>("resize", {
      session,
      cols,
      rows,
      width: width ?? null,
      height: height ?? null,
    });
  }

  watch(session: SessionId, watching: boolean): Promise<void> {
    return call("watch", { session, watching });
  }

  looking(looking: boolean, focused: SessionId | null, seen: boolean): Promise<void> {
    return call("looking", { looking, focused, seen });
  }

  colours(colours: TerminalColours): Promise<void> {
    return call("colours", { colours });
  }

  async listFolders(path: string): Promise<Folders> {
    try {
      return await invoke<Folders>("list_folders", { path });
    } catch (error) {
      // The app's own FolderError: what could not be read, and why.
      const refused = error as { path?: string; reason?: string } | undefined;
      throw new BridgeFailure(
        "refused",
        refused?.reason ? `Cannot read ${refused.path ?? path}: ${refused.reason}` : String(error),
      );
    }
  }

  async pickFolder(title?: string): Promise<PickedFolder> {
    return invoke<PickedFolder>("pick_folder", { title: title ?? null });
  }

  async onFolderDrop(listener: (paths: string[]) => void): Promise<() => void> {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    return getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (payload.type === "drop") listener(payload.paths);
    });
  }
}
