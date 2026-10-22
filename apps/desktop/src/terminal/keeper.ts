// What is kept of the window's terminals between their showings, by
// session: one a screen shows is held by it (a lease), and given back as
// it goes, kept idle for its next showing; the least lately shown idle
// ones beyond a bound let go. A held one is never let go but by `clear`,
// so nothing on screen loses its terminal. No timer and no polling: what
// changes it is a showing, a session gone, or the window going (`ARC-25`).

/** Something kept: let go of everything it holds once disposed. */
export interface Keepable {
  dispose(): void;
}

/** A screen's hold on what is kept for a key; given back once only. */
export interface Lease<T> {
  readonly key: string;
  readonly item: T;
}

interface Entry<T> {
  item: T;
  /** The lease holding it, while a screen shows it. */
  holder: Lease<T> | null;
  /** Its key went while it was held: let go once given back. */
  gone: boolean;
}

export class Keeper<T extends Keepable> {
  private readonly entries = new Map<string, Entry<T>>();
  /** Idle keys, the least lately shown first. */
  private readonly idle: string[] = [];

  constructor(
    private readonly make: (key: string) => T,
    /** How many idle ones are kept at most. */
    private readonly idleLimit: number,
  ) {}

  /**
   * The one kept for `key`, made if none is, held by a new lease. A lease
   * still holding it is taken over: its giving back then does nothing.
   */
  take(key: string): Lease<T> {
    let entry = this.entries.get(key);
    if (!entry) {
      entry = { item: this.make(key), holder: null, gone: false };
      this.entries.set(key, entry);
    }
    this.forgetIdle(key);
    const lease = { key, item: entry.item };
    entry.holder = lease;
    entry.gone = false;
    return lease;
  }

  /** Gives back what `lease` holds: kept idle, or let go if its key went or too many are idle. */
  give(lease: Lease<T>): void {
    const entry = this.entries.get(lease.key);
    if (!entry || entry.holder !== lease) return;
    entry.holder = null;
    if (entry.gone) {
      this.letGo(lease.key);
      return;
    }
    this.idle.push(lease.key);
    while (this.idle.length > this.idleLimit) {
      const oldest = this.idle[0];
      if (oldest === undefined) break;
      this.letGo(oldest);
    }
  }

  /** Lets go of every key `exists` no longer knows: idle at once, held once given back. */
  keepOnly(exists: (key: string) => boolean): void {
    for (const [key, entry] of [...this.entries]) {
      if (exists(key)) continue;
      if (entry.holder) entry.gone = true;
      else this.letGo(key);
    }
  }

  /** Lets go of everything, held or not: the window goes. */
  clear(): void {
    for (const key of [...this.entries.keys()]) this.letGo(key);
  }

  /** Whether anything is kept for `key`, and whether a screen holds it. */
  state(key: string): "none" | "idle" | "held" {
    const entry = this.entries.get(key);
    if (!entry) return "none";
    return entry.holder ? "held" : "idle";
  }

  get size(): number {
    return this.entries.size;
  }

  private letGo(key: string): void {
    const entry = this.entries.get(key);
    this.entries.delete(key);
    this.forgetIdle(key);
    entry?.item.dispose();
  }

  private forgetIdle(key: string): void {
    const at = this.idle.indexOf(key);
    if (at >= 0) this.idle.splice(at, 1);
  }
}
