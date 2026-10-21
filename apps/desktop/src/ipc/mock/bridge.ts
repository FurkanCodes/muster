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
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { SessionId } from "../generated/SessionId";
import type { SessionStatus } from "../generated/SessionStatus";
import type { Workspace } from "../generated/Workspace";
import { handleCommand } from "./commands";
import { derive } from "./model";
import { NOW, type Scenario } from "./scenario";

const encoder = new TextEncoder();

export class MockBridge implements Bridge {
  readonly world: Scenario;
  /** Every command sent, in order: tests look here. */
  readonly sent: Command[] = [];
  private listener: ((message: StateMessage) => void) | null = null;
  private readonly changed = new Map<SessionId, number>();
  private clock = 0;
  private lookingNow = false;
  private readonly watching = new Set<SessionId>();
  private readonly held = new Map<string, { wait: Promise<void>; release: () => void }>();
  private readonly failures = new Map<string, string>();
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
          resources: {
            host_memory_used: 21e9,
            host_memory_total: 34e9,
            sessions: this.world.usage ?? [],
          },
          notifier: false,
        }),
      });
    });
  }

  async command(command: Command): Promise<Reply> {
    this.sent.push(command);
    const key = JSON.stringify(command);
    const held = this.held.get(key);
    if (held) await held.wait;
    const failure = this.failures.get(key);
    if (failure) {
      this.failures.delete(key);
      throw new BridgeFailure("refused", failure, "Internal");
    }
    const reply = handleCommand(this, command);
    if (reply === undefined) {
      const name = typeof command === "string" ? command : Object.keys(command)[0];
      throw new BridgeFailure("invalid", `The mock daemon does not do ${name} yet`);
    }
    return reply;
  }

  async attach(
    session: SessionId,
    _fromSeq: number | null,
    watching: boolean,
    onFrame: (frame: TermFrame) => void,
  ): Promise<void> {
    const text = encoder.encode(this.world.terminals[session] ?? "");
    this.terminals.set(session, { onFrame, seq: text.byteLength });
    if (watching) this.watching.add(session);
    else this.watching.delete(session);
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
    if (watching && this.lookingNow) this.see(session);
  }

  async detach(session: SessionId): Promise<void> {
    this.terminals.delete(session);
    this.watching.delete(session);
  }

  /** Echoes what is typed, as a program at its prompt would. */
  async input(session: SessionId, text: string): Promise<void> {
    this.output(session, text.replace(/\r/g, "\r\n"));
  }

  async resize(): Promise<void> {}
  async watch(session: SessionId, watching: boolean): Promise<void> {
    if (!this.terminals.has(session)) return;
    if (watching) this.watching.add(session);
    else this.watching.delete(session);
    if (watching && this.lookingNow && this.terminals.has(session)) this.see(session);
  }
  async looking(looking: boolean, _focused: SessionId | null = null): Promise<void> {
    this.lookingNow = looking;
    if (looking) for (const id of this.watching) this.see(id);
  }

  /** Watched while the person looks: its news is seen, as the daemon clears it. */
  private see(session: SessionId): void {
    if (!this.world.sessions.find((stored) => stored.id === session)?.unseen) return;
    this.updateSession(session, { unseen: false });
    this.emitSession(session);
  }

  // What tests drive.

  /** Holds back the answer to `command` until `release`, to show a screen waiting. */
  hold(command: Command): void {
    const key = JSON.stringify(command);
    if (this.held.has(key)) return;
    let release = () => {};
    const wait = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.held.set(key, { wait, release });
  }

  /** Answers a held command. */
  release(command: Command): void {
    const key = JSON.stringify(command);
    this.held.get(key)?.release();
    this.held.delete(key);
  }

  /** Refuses the next `command` with `message`, as a daemon that failed at it would. */
  failNext(command: Command, message: string): void {
    this.failures.set(JSON.stringify(command), message);
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

  /** Changes a workspace's fields and tells the page, as when a turn is recorded there. */
  updateWorkspace(id: string, fields: Partial<Workspace>): void {
    const workspace = this.world.workspaces.find((stored) => stored.id === id);
    if (!workspace) return;
    Object.assign(workspace, fields);
    this.emit({ kind: "patch", patch: this.patch({ workspaces: [{ ...workspace }] }) });
  }

  removeSession(session: SessionId): void {
    this.terminals.delete(session);
    this.watching.delete(session);
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
