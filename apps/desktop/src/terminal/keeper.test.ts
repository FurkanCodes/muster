import { describe, expect, it } from "vitest";
import { Keeper } from "./keeper";

class Item {
  disposed = 0;
  constructor(readonly key: string) {}
  dispose() {
    this.disposed += 1;
  }
}

function keeper(idle = 2) {
  const made: Item[] = [];
  const kept = new Keeper<Item>((key) => {
    const item = new Item(key);
    made.push(item);
    return item;
  }, idle);
  return { kept, made };
}

describe("ARC-25 the terminals kept between their showings", () => {
  it("makes a session's terminal once, and gives the same one at its next showing", () => {
    const { kept, made } = keeper();
    const first = kept.take("a");
    kept.give(first);
    const again = kept.take("a");
    expect(again.item).toBe(first.item);
    expect(made).toHaveLength(1);
    expect(kept.state("a")).toBe("held");
  });

  it("never lets go of one a screen holds, however many go idle", () => {
    const { kept } = keeper(1);
    const shown = kept.take("shown");
    for (const key of ["b", "c", "d", "e"]) kept.give(kept.take(key));
    expect(kept.state("shown")).toBe("held");
    expect(shown.item.disposed).toBe(0);
  });

  it("keeps the most lately shown idle ones only, letting go of the least first", () => {
    const { kept, made } = keeper(2);
    for (const key of ["a", "b", "c"]) kept.give(kept.take(key));
    expect(kept.state("a")).toBe("none");
    expect(made[0]?.disposed).toBe(1);
    // b shown again is the latest: c goes next, not b.
    kept.give(kept.take("b"));
    kept.give(kept.take("d"));
    expect(kept.state("b")).toBe("idle");
    expect(kept.state("c")).toBe("none");
    expect(kept.state("d")).toBe("idle");
  });

  it("lets a new showing take over, the old one's giving back doing nothing", () => {
    const { kept } = keeper();
    const old = kept.take("a");
    const now = kept.take("a");
    kept.give(old);
    expect(kept.state("a")).toBe("held");
    kept.give(now);
    expect(kept.state("a")).toBe("idle");
    kept.give(now);
    expect(kept.state("a")).toBe("idle");
  });

  it("lets go of a removed session's at once if idle, and once given back if shown", () => {
    const { kept } = keeper();
    kept.give(kept.take("idle"));
    const shown = kept.take("shown");
    kept.keepOnly(() => false);
    expect(kept.state("idle")).toBe("none");
    expect(kept.state("shown")).toBe("held");
    expect(shown.item.disposed).toBe(0);
    kept.give(shown);
    expect(kept.state("shown")).toBe("none");
    expect(shown.item.disposed).toBe(1);
  });

  it("lets go of all of them, shown or not, as the window goes", () => {
    const { kept, made } = keeper();
    kept.give(kept.take("a"));
    const shown = kept.take("b");
    kept.clear();
    expect(kept.size).toBe(0);
    expect(made.map((item) => item.disposed)).toEqual([1, 1]);
    // Given back after, it is not kept again.
    kept.give(shown);
    expect(kept.size).toBe(0);
  });
});
