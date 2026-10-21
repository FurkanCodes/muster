// A pretend daemon in the page, behind the same Bridge the real one has.
// `pnpm dev:mock`, the unit tests and the Playwright tests run against it.
// Pages that send commands it does not know yet teach it in `commands.ts`.

import { type Bridge, BridgeFailure } from "../bridge";
import {
  decodeFrame,
  encodeFrame,
  FRAME_EVENT,
  FRAME_OUTPUT,
  FRAME_REPLAY,
  type TermFrame,
} from "../frames";
import type { Command } from "../generated/Command";
import type { LinkState } from "../generated/desktop/LinkState";
import type { Patch } from "../generated/desktop/Patch";
import type { StateMessage } from "../generated/desktop/StateMessage";
import type { ErrorCode } from "../generated/ErrorCode";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { SessionId } from "../generated/SessionId";
import type { SessionStatus } from "../generated/SessionStatus";
import { handleCommand } from "./commands";
import { derive } from "./model";
import { NOW, type Scenario } from "./scenario";
import { agentsOnTickets } from "./tickets";

const encoder = new TextEncoder();

export class MockBridge implements Bridge {
  readonly world: Scenario;
  /** Every command sent, in order: tests look here. */
  readonly sent: Command[] = [];
  readonly openedUrls: string[] = [];
  private readonly holds = new Map<string, { promise: Promise<void>; release: () => void }>();
  private readonly failures = new Map<string, BridgeFailure>();
  private listener: ((message: StateMessage) => void) | null = null;
  private readonly changed = new Map<SessionId, number>();
  private clock = 0;
  private readonly terminals = new Map<
    SessionId,
    { onFrame: (frame: TermFrame) => void; seq: number }
  >();

  constructor(world: Scenario) {
    this.world = world;
    for (const session of world.sessions) this.changed.set(session.id, this.tick());
  }

  private tick(): number {
    this.clock += 1;
    return this.clock;
  }

  private emit(message: StateMessage): void {
    this.listener?.(message);
  }

  private patch(parts: Partial<Patch>): Patch {
    return {
      reset: false,
      projects: [],
      workspaces: [],
      sessions: [],
      gone: { projects: [], workspaces: [], sessions: [] },
      preferences: null,
      plugins: null,
      resources: null,
      notifier: null,
      ...parts,
      derived: derive(this.world, this.changed),
    };
  }

  async open(onMessage: (message: StateMessage) => void): Promise<void> {
    this.listener = onMessage;
    this.emit({ kind: "link", link: { state: "connecting" } });
    queueMicrotask(() => {
      this.emit({ kind: "link", link: { state: "up", version: "0.4.1", protocol: 44 } });
      this.emit({
        kind: "patch",
        patch: this.patch({
          reset: true,
          projects: [...this.world.projects],
          workspaces: [...this.world.workspaces],
          sessions: [...this.world.sessions],
          preferences: this.world.preferences,
          plugins: this.world.plugins,
          resources: { host_memory_used: 21e9, host_memory_total: 34e9, sessions: [] },
          notifier: false,
        }),
      });
    });
  }

  async command(command: Command): Promise<Reply> {
    this.sent.push(command);
    const name = typeof command === "string" ? command : (Object.keys(command)[0] ?? "");
    await this.holds.get(name)?.promise;
    const failure = this.failures.get(name);
    if (failure) {
      this.failures.delete(name);
      throw failure;
    }
    const reply = handleCommand(this, command);
    if (reply === undefined) {
      const name = typeof command === "string" ? command : Object.keys(command)[0];
      throw new BridgeFailure("invalid", `The mock daemon does not do ${name} yet`);
    }
    return reply;
  }

  async openUrl(url: string): Promise<void> {
    let parsed: URL;
    try {
      parsed = new URL(url);
    } catch {
      throw new BridgeFailure("invalid", "This ticket has no web address.");
    }
    if (!["http:", "https:"].includes(parsed.protocol))
      throw new BridgeFailure("invalid", "Only http and https ticket addresses can be opened.");
    // Browser tests record the explicit request without reaching a real tracker.
    this.openedUrls.push(parsed.href);
  }

  async attach(
    session: SessionId,
    _fromSeq: number | null,
    _watching: boolean,
    onFrame: (frame: TermFrame) => void,
  ): Promise<void> {
    const text = encoder.encode(this.world.terminals[session] ?? "");
    this.terminals.set(session, { onFrame, seq: text.byteLength });
    onFrame(decodeFrame(encodeFrame(FRAME_REPLAY, 0, text)));
    const modes = {
      kind: "modes",
      modes: {
        kitty_flags: 0,
        bracketed_paste: true,
        application_cursor: false,
        mouse: "off",
        mouse_encoding: "default",
        alternate_screen: false,
      },
    };
    onFrame(decodeFrame(encodeFrame(FRAME_EVENT, 0, encoder.encode(JSON.stringify(modes)))));
  }

  async detach(session: SessionId): Promise<void> {
    this.terminals.delete(session);
  }

  /** Echoes what is typed, as a program at its prompt would. */
  async input(session: SessionId, text: string): Promise<void> {
    this.output(session, text.replace(/\r/g, "\r\n"));
  }

  async resize(): Promise<void> {}
  async watch(): Promise<void> {}
  async looking(): Promise<void> {}

  // What tests drive.

  /** Holds the replies to `command` until {@link release}, as a slow tracker would. */
  hold(command: string): void {
    let release = () => {};
    const promise = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.holds.set(command, { promise, release });
  }

  release(command: string): void {
    this.holds.get(command)?.release();
    this.holds.delete(command);
  }

  /** Refuses the next `command` as the daemon would, with its message and code. */
  failNext(command: string, text: string, code: ErrorCode = "internal"): void {
    this.failures.set(command, new BridgeFailure("refused", text, code));
  }

  /** Tells the page of changed entities, as a delta would. */
  publish(parts: Partial<Patch>): void {
    this.emit({ kind: "patch", patch: this.patch(parts) });
  }

  /** Puts the agents of the Tickets board on their tickets (`tickets.ts`). */
  agentsOnTickets(): void {
    agentsOnTickets(this);
  }

  /** Writes `text` to a session's terminal, as its program would. */
  output(session: SessionId, text: string): void {
    const terminal = this.terminals.get(session);
    if (!terminal) return;
    const bytes = encoder.encode(text);
    terminal.onFrame(decodeFrame(encodeFrame(FRAME_OUTPUT, terminal.seq, bytes)));
    terminal.seq += bytes.byteLength;
  }

  /** Puts a session in another status, as the daemon would tell. */
  setStatus(session: SessionId, status: SessionStatus, unseen = false): void {
    this.updateSession(session, { status, unseen, status_since: NOW });
    this.changed.set(session, this.tick());
    this.emitSession(session);
  }

  /** Changes a session's fields and tells the page. */
  updateSession(session: SessionId, fields: Partial<Session>): void {
    const found = this.world.sessions.find((s) => s.id === session);
    if (found) Object.assign(found, fields);
  }

  emitSession(session: SessionId): void {
    const found = this.world.sessions.find((s) => s.id === session);
    if (found) this.emit({ kind: "patch", patch: this.patch({ sessions: [{ ...found }] }) });
  }

  removeSession(session: SessionId): void {
    this.world.sessions = this.world.sessions.filter((s) => s.id !== session);
    this.changed.delete(session);
    this.emit({
      kind: "patch",
      patch: this.patch({ gone: { projects: [], workspaces: [], sessions: [session] } }),
    });
  }

  setLink(link: LinkState): void {
    this.emit({ kind: "link", link });
  }

  reveal(session: SessionId): void {
    this.emit({ kind: "reveal", session });
  }
}
