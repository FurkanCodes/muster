// A terminal drawn on the GPU (xterm's WebGL renderer) while it is shown,
// and only then: given back, it lets go of its WebGL context at once (lost
// on purpose, not left for the collector), so the window holds one for
// each terminal on screen, as WebKit limits them. A context lost (after a
// sleep, under memory pressure; xterm reports it once its own wait for the
// context to come back has run out) or one that could not be had is tried
// again on the next frame, a few times a showing; after that xterm draws
// with the DOM, says so once, and the next showing tries afresh. Frames,
// not timers (`ARC-25`), and only while the terminal is shown and the
// window seen: one lost while the window is hidden is tried again once it
// is seen, and a terminal shown keeps its context while the window is
// hidden, so coming back draws as it left.

/** As much of xterm's WebGL addon as is needed here. */
export interface GpuAddon {
  dispose(): void;
  onContextLoss(listener: () => void): { dispose(): void };
}

/** A WebGL context to let go of, as `WEBGL_lose_context` does. */
export interface HeldContext {
  loseContext(): void;
}

export interface Frames {
  request(callback: () => void): number;
  cancel(frame: number): void;
}

/** Tries in one showing after the first, a lost context or a failed load each one. */
export const MOST_RETRIES = 3;

export class Gpu {
  private addon: GpuAddon | null = null;
  private wanted = false;
  private seen = true;
  private failures = 0;
  private frame: number | null = null;
  private said = false;

  constructor(
    /** Makes an addon and loads it into the terminal; throws where no WebGL context can be had. */
    private readonly load: () => GpuAddon,
    /** The contexts the terminal draws with now, let go of once its addon is. */
    private readonly contexts: () => HeldContext[],
    private readonly frames: Frames,
    private readonly say: (words: string) => void = (words) => console.warn(words),
  ) {}

  /** Drawn on the GPU while `on` (shown and seen); given back, not. */
  want(on: boolean): void {
    if (on === this.wanted) return;
    this.wanted = on;
    if (on) {
      // A new showing tries afresh.
      this.failures = 0;
      this.said = false;
      this.start();
    } else {
      this.cancelFrame();
      this.drop();
    }
  }

  /** Whether the window is seen: only then is a lost context tried again. */
  see(seen: boolean): void {
    if (seen === this.seen) return;
    this.seen = seen;
    if (!seen) this.cancelFrame();
    else if (this.wanted && !this.addon && this.failures <= MOST_RETRIES) this.start();
  }

  /** Whether the GPU draws it now. */
  get drawing(): boolean {
    return this.addon !== null;
  }

  /** Whether a frame is due to try again. */
  get retrying(): boolean {
    return this.frame !== null;
  }

  dispose(): void {
    this.want(false);
  }

  private start(): void {
    if (this.addon || !this.wanted || !this.seen) return;
    let addon: GpuAddon;
    try {
      addon = this.load();
    } catch {
      this.failed("no WebGL context could be had");
      return;
    }
    this.addon = addon;
    const lost = addon.onContextLoss(() => {
      lost.dispose();
      if (this.addon !== addon) return;
      this.drop();
      this.failed("its WebGL context was lost");
    });
  }

  private failed(why: string): void {
    this.failures += 1;
    if (this.failures > MOST_RETRIES) {
      if (!this.said)
        this.say(`The terminal is drawn with the DOM until it is shown again: ${why}.`);
      this.said = true;
      return;
    }
    if (!this.wanted || !this.seen || this.frame !== null) return;
    this.frame = this.frames.request(() => {
      this.frame = null;
      this.start();
    });
  }

  private drop(): void {
    const addon = this.addon;
    if (!addon) return;
    this.addon = null;
    const held = this.contexts();
    addon.dispose();
    for (const context of held) context.loseContext();
  }

  private cancelFrame(): void {
    if (this.frame === null) return;
    this.frames.cancel(this.frame);
    this.frame = null;
  }
}
