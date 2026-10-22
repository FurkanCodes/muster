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
import type { Folders } from "../generated/desktop/Folders";
import type { LinkState } from "../generated/desktop/LinkState";
import type { Patch } from "../generated/desktop/Patch";
import type { StateMessage } from "../generated/desktop/StateMessage";
import type { ErrorCode } from "../generated/ErrorCode";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { SessionId } from "../generated/SessionId";
import type { SessionStatus } from "../generated/SessionStatus";
import type { TerminalColours } from "../generated/TerminalColours";
import type { Workspace } from "../generated/Workspace";
import { ByHandWorld } from "./byHand";
import { handleCommand } from "./commands";
import { FlowWorld } from "./flows";
import { expandPath } from "./folders";
import { derive } from "./model";
import { NOW, type Scenario } from "./scenario";
import { MockSettings } from "./settings";
import { agentsOnTickets } from "./tickets";

const encoder = new TextEncoder();

/** A command's kind: its variant's name. */
function nameOf(command: Command): string {
  return typeof command === "string" ? command : (Object.keys(command)[0] ?? "");
}

/** What `hold` and `failNext` file a command under: a kind by its name, or one whole command. */
function keyOf(command: string | Command): string {
  return typeof command === "string" ? `name:${command}` : `command:${JSON.stringify(command)}`;
}

/** Every key a command sent answers to. */
function keysOf(command: Command): string[] {
  return [`name:${nameOf(command)}`, `command:${JSON.stringify(command)}`];
}

/** The commands that make something, whose change the daemon tells after its answer. */
const MAKES = new Set([
  "AddProject",
  "CreateWorktree",
  "StartSession",
  "TidyCommits",
  "StartFromIssue",
]);

export class MockBridge implements Bridge {
  readonly world: Scenario;
  /** What git and the file system say, for the flows (flows.ts). */
  readonly flows: FlowWorld;
  /** What the projects' settings and the host say, for the stages by hand (byHand.ts). */
  readonly byHand: ByHandWorld;
  private readonly drops = new Set<(paths: string[]) => void>();
  /** Every command sent, in order: tests look here. */
  readonly sent: Command[] = [];
  readonly openedUrls: string[] = [];
  private readonly holds = new Map<
    string,
    { promise: Promise<void>; release: () => void; done: "later" | "now" }
  >();
  private readonly failures = new Map<string, BridgeFailure>();
  /** Every one of Settings' asks of muster and the Mac, in order (`screens/settings/asks.ts`). */
  readonly asked: string[] = [];
  /** How often the page asked to reach the daemon now, and to open its log. */
  triedNow = 0;
  logsShown = 0;
  /** Every sound asked to be heard, in order. */
  readonly heard: string[] = [];
  /** Its settings and plugins, and what tests drive of them. */
  readonly settings: MockSettings;
  private listener: ((message: StateMessage) => void) | null = null;
  private readonly changed = new Map<SessionId, number>();
  private clock = 0;
  /** Whether the window can be seen, as the page last said: questions only then. */
  private seen = true;
  private lookingNow = false;
  private readonly watching = new Set<SessionId>();
  private readonly terminals = new Map<
    SessionId,
    { onFrame: (frame: TermFrame) => void; seq: number }
  >();

  constructor(world: Scenario) {
    this.world = world;
    this.flows = new FlowWorld(world);
    this.byHand = new ByHandWorld(world.projects.map((project) => project.id));
    for (const session of world.sessions) this.changed.set(session.id, this.tick());
    this.settings = new MockSettings(this);
  }

  private tick(): number {
    this.clock += 1;
    return this.clock;
  }

  /** What a command that makes something tells, held until after its answer (see `command`). */
  private told: StateMessage[] | null = null;

  private emit(message: StateMessage): void {
    if (this.told) this.told.push(message);
    else this.listener?.(message);
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
      derived: derive(this.world, this.changed, this.seen),
    };
  }

  async open(onMessage: (message: StateMessage) => void): Promise<void> {
    this.listener = onMessage;
    this.emit({ kind: "link", link: { state: "connecting" } });
    queueMicrotask(() => {
      this.emit({ kind: "link", link: { state: "up", version: "0.4.1", protocol: 47 } });
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

  /** The pretend daemon is never out of reach but as a test says (`setLink`): asked, it counts. */
  async tryNow(): Promise<void> {
    this.triedNow += 1;
  }

  async showDaemonLog(): Promise<void> {
    this.logsShown += 1;
  }

  /** The terminals' colours the page last told, as the daemon keeps them. */
  toldColours: TerminalColours | null = null;

  async colours(colours: TerminalColours): Promise<void> {
    this.toldColours = colours;
  }

  async command(command: Command): Promise<Reply> {
    this.sent.push(command);
    for (const key of keysOf(command)) {
      const held = this.holds.get(key);
      if (held?.done === "later") await held.promise;
    }
    const key = keysOf(command).find((key) => this.failures.has(key));
    const failure = key === undefined ? undefined : this.failures.get(key);
    if (key !== undefined && failure) {
      this.failures.delete(key);
      throw failure;
    }
    // The daemon answers a command that makes something before the change
    // it made reaches its clients: the mock tells it after its answer too.
    const makes = MAKES.has(nameOf(command));
    if (makes) this.told = [];
    let reply: Reply | undefined;
    try {
      reply = handleCommand(this, command);
    } finally {
      const told = this.told ?? [];
      this.told = null;
      if (told.length > 0)
        setTimeout(() => {
          for (const message of told) this.emit(message);
        }, 0);
    }
    if (reply === undefined) {
      const name = typeof command === "string" ? command : Object.keys(command)[0];
      throw new BridgeFailure("invalid", `The mock daemon does not do ${name} yet`);
    }
    // A copy, as one that came over the wire: the page never holds the mock's own objects.
    const answer = structuredClone(reply);
    for (const key of keysOf(command)) {
      const held = this.holds.get(key);
      if (held?.done === "now") await held.promise;
    }
    return answer;
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

  /** Plays a sound by its name, as the app's Rust side would: the mock records it. */
  async restartInPlace(): Promise<string> {
    this.asked.push("restart in place");
    return "The daemon is now muster 0.4.1, its sessions running on.";
  }

  async openNotificationSettings(): Promise<void> {
    this.asked.push("notification settings");
  }

  async editSettingsFile(): Promise<void> {
    this.asked.push("edit settings.toml");
  }

  async hear(sound: string): Promise<void> {
    this.heard.push(sound);
  }

  async resize(): Promise<void> {}
  async watch(session: SessionId, watching: boolean): Promise<void> {
    if (!this.terminals.has(session)) return;
    if (watching) this.watching.add(session);
    else this.watching.delete(session);
    if (watching && this.lookingNow && this.terminals.has(session)) this.see(session);
  }
  /** As the Rust side, too: questions are read only while the window is seen. */
  async looking(looking: boolean, _focused: SessionId | null = null, seen = true): Promise<void> {
    this.lookingNow = looking;
    if (looking) for (const id of this.watching) this.see(id);
    if (seen === this.seen) return;
    this.seen = seen;
    if (seen) this.emit({ kind: "patch", patch: this.patch({}) });
  }

  /** Watched while the person looks: its news is seen, as the daemon clears it. */
  private see(session: SessionId): void {
    if (!this.world.sessions.find((stored) => stored.id === session)?.unseen) return;
    this.updateSession(session, { unseen: false });
    this.emitSession(session);
  }

  async listFolders(typed: string): Promise<Folders> {
    const path = expandPath(typed);
    const why = this.flows.unreadable[path];
    if (why) throw new BridgeFailure("refused", `Cannot read ${path}: ${why}`);
    if (!path.startsWith("/"))
      throw new BridgeFailure("refused", `Cannot read ${path}: start it with / or ~/`);
    const folders = this.flows.folders[path];
    if (!folders)
      throw new BridgeFailure(
        "refused",
        `Cannot read ${path}: No such file or directory (os error 2)`,
      );
    return { path, folders };
  }

  async onFolderDrop(listener: (paths: string[]) => void): Promise<() => void> {
    this.drops.add(listener);
    return () => this.drops.delete(listener);
  }

  // What tests drive.

  /** Drops `paths` on the window, as Finder would. */
  drop(paths: string[]): void {
    for (const listener of this.drops) listener(paths);
  }

  /**
   * Holds the replies to `command` until {@link release}, as a slow daemon or
   * tracker would: a command's name holds every one of its kind, a whole
   * command only that one. Done `"later"`, the command is done as it is
   * released; done `"now"`, it is done at once and its answer, as things
   * stood then, comes as it is released, as the daemon answers a
   * `ChangeSetting` from a task of its own, after any sent since.
   */
  hold(command: string | Command, done: "later" | "now" = "later"): void {
    const key = keyOf(command);
    if (this.holds.has(key)) return;
    let release = () => {};
    const promise = new Promise<void>((resolve) => {
      release = resolve;
    });
    this.holds.set(key, { promise, release, done });
  }

  /** Answers a held command. */
  release(command: string | Command): void {
    const key = keyOf(command);
    this.holds.get(key)?.release();
    this.holds.delete(key);
  }

  /** Refuses the next `command` (a name, or a whole command) as the daemon would, with its message and code. */
  failNext(command: string | Command, text: string, code: ErrorCode = "internal"): void {
    this.failures.set(keyOf(command), new BridgeFailure("refused", text, code));
  }

  /** Tells the page of changed entities, as a delta would. */
  publish(parts: Partial<Patch>): void {
    this.emit({ kind: "patch", patch: this.patch(parts) });
  }

  /** Puts the agents of the Tickets board on their tickets (`tickets.ts`). */
  agentsOnTickets(): void {
    agentsOnTickets(this);
  }

  /**
   * Writes `text` to a session's terminal, as its program would. A session
   * asking the person shows it in its question.
   */
  output(session: SessionId, text: string): void {
    this.world.terminals[session] = (this.world.terminals[session] ?? "") + text;
    const asking = this.world.sessions.some(
      (found) => found.id === session && found.status.state === "needs_you",
    );
    if (asking && this.seen) this.emit({ kind: "patch", patch: this.patch({}) });
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
