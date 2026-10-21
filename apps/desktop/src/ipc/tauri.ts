// The real bridge: the app's Rust side, through Tauri's IPC. State comes on
// one ordered channel; each attached terminal has its own channel of raw
// byte frames.

import { Channel, invoke } from "@tauri-apps/api/core";
import { type Bridge, BridgeFailure } from "./bridge";
import { decodeFrame, type TermFrame } from "./frames";
import type { Command } from "./generated/Command";
import type { BridgeError } from "./generated/desktop/BridgeError";
import type { StateMessage } from "./generated/desktop/StateMessage";
import type { Reply } from "./generated/Reply";
import type { SessionId } from "./generated/SessionId";

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

  command(command: Command): Promise<Reply> {
    return call<Reply>("command", { command });
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

  looking(looking: boolean, focused: SessionId | null): Promise<void> {
    return call("looking", { looking, focused });
  }
}
