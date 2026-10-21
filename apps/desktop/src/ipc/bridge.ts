// The one door between the webview and the daemon. The app talks only to a
// Bridge: the real one goes through the app's Rust side (`tauri.ts`), the
// mock one runs a pretend daemon in the page (`mock/`) for development and
// tests. Pages never call `invoke` themselves.

import type { TermFrame } from "./frames";
import type { Command } from "./generated/Command";
import type { Folders } from "./generated/desktop/Folders";
import type { StateMessage } from "./generated/desktop/StateMessage";
import type { Reply } from "./generated/Reply";
import type { SessionId } from "./generated/SessionId";
import type { TerminalColours } from "./generated/TerminalColours";

export interface Bridge {
  /** Opens the link. Messages arrive on `onMessage`, in order, until `close`. */
  open(onMessage: (message: StateMessage) => void): Promise<void>;
  /** Tries to reach the daemon now, starting it again if nothing answers, while it is out of reach. */
  tryNow(): Promise<void>;
  /** Opens the daemon's log (daemon.log in Muster's runtime folder). Rejects with why it cannot. */
  showDaemonLog(): Promise<void>;
  /** Sends a command to the daemon. Rejects with a {@link BridgeFailure}. */
  command(command: Command): Promise<Reply>;
  /** Opens a ticket's HTTP(S) address in the person's browser, only on request. */
  openUrl(url: string): Promise<void>;
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
  /** The colours the window's terminals draw in, for programs that ask their terminal's (`ClientFrame::Colours`). */
  colours(colours: TerminalColours): Promise<void>;
  /** The folders in the folder `path` (absolute, or from `~`), to add one as a project. Rejects with why it cannot. */
  listFolders(path: string): Promise<Folders>;
  /** Tells `listener` the paths of what is dropped on the window, until the returned function is called. */
  onFolderDrop(listener: (paths: string[]) => void): Promise<() => void>;
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
