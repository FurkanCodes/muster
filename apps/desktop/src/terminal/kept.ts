// Each session's xterm, kept between its showings (keeper.ts), so moving a
// session's terminal (alone or beside its shell, the changes and back,
// Tickets and back, another session and back) neither makes it afresh nor
// replays its program's output into it: shown again, it streams only what
// came while it was away. While not shown it is out of the page, streams
// nothing and draws nothing, and holds no WebGL context (`ARC-25`); its
// keys, wheel and modes are its own (wheel.ts). The window keeps
// `IDLE_TERMINALS` not on screen at most, lets go of a removed session's,
// and of all of them as the page goes.

import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { type ITheme, Terminal as XTerm } from "@xterm/xterm";
import type { App } from "../app/context";
import type { Bridge } from "../ipc/bridge";
import type { TermFrame } from "../ipc/frames";
import type { SessionId } from "../ipc/generated/SessionId";
import type { TerminalModes } from "../ipc/generated/TerminalModes";
import { Gpu, type HeldContext } from "./gpu";
import { Keeper } from "./keeper";
import { exposeForTests } from "./testing";
import { WheelLines, wheelReport } from "./wheel";

/** Terminals not on screen kept for their next showing, the least lately shown let go first. */
export const IDLE_TERMINALS = 4;

const FACE = '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace';

function sizing(compact: boolean) {
  return { fontSize: compact ? 12 : 13, lineHeight: compact ? 1.5 : 1.54 };
}

/** The modes a program set, written to xterm itself so its keys and pastes match. */
function modeSequences(modes: TerminalModes): string {
  const set = (on: boolean, mode: number) => `\x1b[?${mode}${on ? "h" : "l"}`;
  return set(modes.application_cursor, 1) + set(modes.bracketed_paste, 2004);
}

/**
 * Loads a WebGL addon into `xterm`. xterm lists an addon before it starts
 * it, so one that cannot start (no context to be had) is disposed, which
 * takes it off the list, before the failure goes on: failed tries never
 * pile up in a terminal kept for long.
 */
export function loadWebgl<A extends { dispose(): void }>(
  xterm: { loadAddon(addon: NoInfer<A>): void },
  make: () => A,
): A {
  const addon = make();
  try {
    xterm.loadAddon(addon);
  } catch (error) {
    try {
      addon.dispose();
    } catch {
      // Half started: what it holds goes with the page.
    }
    throw error;
  }
  return addon;
}

/** A stream feeding a terminal: its frames, while it is the terminal's own. */
export interface Feed {
  onFrame(frame: TermFrame): void;
}

export class KeptTerminal {
  readonly xterm: XTerm;
  readonly fit = new FitAddon();
  readonly gpu: Gpu;
  /** The position after the last byte written, so an attach asks only for what is new. */
  nextSeq: number | null = null;
  /** The modes the program set, as the daemon last said: whether it takes the mouse. */
  modes: TerminalModes | null = null;
  /** The size its program was last told, as `colsxrows`. */
  told = "";
  private feed: Feed | null = null;
  private compact: boolean;
  private disposed = false;
  private readonly lets: (() => void)[] = [];

  constructor(
    bridge: Bridge,
    readonly session: SessionId,
    compact: boolean,
  ) {
    this.compact = compact;
    const xterm = new XTerm({
      fontFamily: FACE,
      ...sizing(compact),
      cursorBlink: false,
      scrollback: 5000,
      allowProposedApi: false,
      macOptionIsMeta: false,
    });
    this.xterm = xterm;
    xterm.loadAddon(this.fit);
    const typed = xterm.onData((text) => void bridge.input(session, text).catch(() => {}));
    this.lets.push(() => typed.dispose());
    xterm.attachCustomKeyEventHandler((event) => {
      // ⌘ shortcuts and ⇧Q (leave the terminal) are Muster's, never the program's.
      if (event.metaKey) return false;
      if (event.type === "keydown" && event.shiftKey && event.key === "Q") return false;
      return true;
    });
    // A program that takes the mouse is sent the wheel's every line, where
    // the pointer is (`wheel.ts`); otherwise xterm scrolls its own lines,
    // or on the alternate screen sends arrow keys.
    const wheelLines = new WheelLines();
    xterm.attachCustomWheelEventHandler((event) => {
      const now = this.modes;
      if (!now || now.mouse === "off" || event.shiftKey) return true;
      const screen = xterm.element?.querySelector(".xterm-screen")?.getBoundingClientRect();
      if (!screen || screen.width === 0 || screen.height === 0) return true;
      event.preventDefault();
      const lines = wheelLines.take(event, {
        cellHeight: screen.height / xterm.rows,
        rows: xterm.rows,
      });
      if (lines === 0) return false;
      const cell = (at: number, size: number, count: number) =>
        Math.min(count - 1, Math.max(0, Math.floor((at / size) * count)));
      const column = cell(event.clientX - screen.left, screen.width, xterm.cols);
      const row = cell(event.clientY - screen.top, screen.height, xterm.rows);
      const report = wheelReport(lines < 0, event, column, row, now);
      if (report) void bridge.input(session, report.repeat(Math.abs(lines))).catch(() => {});
      return false;
    });
    this.gpu = new Gpu(
      () => loadWebgl(xterm, () => new WebglAddon()),
      () => this.contexts(),
      {
        request: (callback) => requestAnimationFrame(callback),
        cancel: (frame) => cancelAnimationFrame(frame),
      },
    );
    this.lets.push(exposeForTests(session, xterm));
  }

  /**
   * Puts it in `host`, opened there the first time, at the size and in the
   * colours it is drawn in there.
   */
  showIn(host: HTMLElement, compact: boolean, theme: ITheme): void {
    if (this.disposed) return;
    this.xterm.options.theme = theme;
    this.setCompact(compact);
    if (!this.xterm.element) {
      this.xterm.open(host);
      // Out of Tab's way: Tab goes round the tabs, as in the terminal UI, and
      // a terminal takes the keys only on ⏎ or a click, never by Tab.
      this.xterm.textarea?.setAttribute("tabindex", "-1");
    } else if (this.xterm.element.parentElement !== host) {
      host.append(this.xterm.element);
    }
  }

  /** Takes it out of the page, its stream and its GPU let go: kept for its next showing. */
  putAway(): void {
    this.stopFeed();
    this.gpu.want(false);
    this.xterm.blur();
    this.xterm.element?.remove();
  }

  /** Drawn as a panel's terminal (smaller) or the agent's, in place: never made again for it. */
  setCompact(compact: boolean): void {
    if (compact === this.compact) return;
    this.compact = compact;
    const { fontSize, lineHeight } = sizing(compact);
    this.xterm.options.fontSize = fontSize;
    this.xterm.options.lineHeight = lineHeight;
  }

  /**
   * Feeds it from a new stream: the one before is let go, and a frame from
   * it that comes late is dropped, as is any once it is disposed.
   */
  startFeed(): Feed {
    const feed: Feed = {
      onFrame: (frame) => {
        if (this.feed !== feed || this.disposed) return;
        this.write(frame);
      },
    };
    this.feed = feed;
    return feed;
  }

  /** Lets go of its stream (`feed`, if only that one): a frame from it that comes late is dropped. */
  stopFeed(feed?: Feed): void {
    if (feed === undefined || this.feed === feed) this.feed = null;
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.stopFeed();
    this.gpu.dispose();
    for (const letGo of this.lets.splice(0)) letGo();
    this.xterm.dispose();
  }

  get isDisposed(): boolean {
    return this.disposed;
  }

  private write(frame: TermFrame): void {
    const xterm = this.xterm;
    switch (frame.kind) {
      case "replay":
        if (frame.reset || this.nextSeq === null) xterm.reset();
        xterm.write(frame.data);
        this.nextSeq = frame.seq + frame.data.byteLength;
        break;
      case "output":
        xterm.write(frame.data);
        this.nextSeq = frame.seq + frame.data.byteLength;
        break;
      // After a replay starting over, whose bytes were cut from the
      // middle of the output: the program's screen, drawn whole.
      case "screen":
        xterm.write(frame.data);
        break;
      case "event":
        if (frame.event.kind === "modes") {
          this.modes = frame.event.modes;
          xterm.write(modeSequences(frame.event.modes));
        }
        break;
    }
  }

  /** The WebGL context it draws with: its one canvas that is no 2D layer's. */
  private contexts(): HeldContext[] {
    const held: HeldContext[] = [];
    for (const canvas of this.xterm.element?.querySelectorAll("canvas") ?? []) {
      if ([...canvas.classList].some((name) => name.endsWith("-layer"))) continue;
      const lose = canvas.getContext("webgl2")?.getExtension("WEBGL_lose_context");
      if (lose) held.push(lose);
    }
    return held;
  }
}

/** The window's kept terminals, one keeper an app. */
const keepers = new WeakMap<App, Keeper<KeptTerminal>>();

/** The keeper of `app`'s terminals, made with its first terminal. */
export function keptTerminals(app: App): Keeper<KeptTerminal> {
  let keeper = keepers.get(app);
  if (keeper) return keeper;
  const made = new Keeper<KeptTerminal>(
    // Made at the agent's size: `showIn` sets the size it is shown at before it opens.
    (session) => new KeptTerminal(app.bridge, session, false),
    IDLE_TERMINALS,
  );
  keeper = made;
  keepers.set(app, made);
  // A session removed takes its terminal with it; the page going, all of them.
  const prune = (sessions: Record<string, unknown>, loaded: boolean) => {
    if (loaded) made.keepOnly((session) => session in sessions);
  };
  const unsubscribe = app.daemon.store.subscribe((state) => prune(state.sessions, state.loaded));
  const gone = () => {
    unsubscribe();
    window.removeEventListener("pagehide", gone);
    made.clear();
  };
  window.addEventListener("pagehide", gone);
  return made;
}
