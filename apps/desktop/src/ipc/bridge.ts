// The one door between the webview and the daemon. The app talks only to a
// Bridge: the real one goes through the app's Rust side (`tauri.ts`), the
// mock one runs a pretend daemon in the page (`mock/`) for development and
// tests. Pages never call `invoke` themselves.

import type { TermFrame } from "./frames";
import type { Command } from "./generated/Command";
import type { StateMessage } from "./generated/desktop/StateMessage";
import type { Reply } from "./generated/Reply";
import type { SessionId } from "./generated/SessionId";

export interface Bridge {
  /** Opens the link. Messages arrive on `onMessage`, in order, until `close`. */
  open(onMessage: (message: StateMessage) => void): Promise<void>;
  /** Sends a command to the daemon. Rejects with a {@link BridgeFailure}. */
  command(command: Command): Promise<Reply>;
  /** Streams a session's terminal on `onFrame`, from `fromSeq` or all the daemon holds. */
  attach(
    session: SessionId,
    fromSeq: number | null,
    watching: boolean,
    onFrame: (frame: TermFrame) => void,
  ): Promise<void>;
  detach(session: SessionId): Promise<void>;
  /** Keystrokes or pasted text, as typed. */
  input(session: SessionId, text: string): Promise<void>;
  resize(
    session: SessionId,
    cols: number,
    rows: number,
    width?: number,
    height?: number,
  ): Promise<void>;
  watch(session: SessionId, watching: boolean): Promise<void>;
  /**
   * Whether the person looks at this window (seen and in focus), which
   * terminal is in focus in it, and whether it can be seen at all: while it
   * cannot, the app's Rust side sends nothing and reads no question.
   */
  looking(looking: boolean, focused: SessionId | null, seen: boolean): Promise<void>;
}

/** Why a bridge call failed, as the Rust side says it (`BridgeError`). */
export class BridgeFailure extends Error {
  constructor(
    readonly kind: "unreachable" | "refused" | "invalid",
    message: string,
    readonly code?: string,
  ) {
    super(message);
    this.name = "BridgeFailure";
  }
}
