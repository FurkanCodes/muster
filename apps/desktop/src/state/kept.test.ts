import { beforeEach, describe, expect, it, vi } from "vitest";
import { keep, keepIn, kept } from "./kept";

describe("the window's own state (ARC-24)", () => {
  beforeEach(() => localStorage.clear());

  it("is kept in the page's storage where nothing else keeps it", () => {
    keep("muster.theme", "light");
    expect(localStorage.getItem("muster.theme")).toBe("light");
    expect(kept("muster.theme")).toBe("light");
    keep("muster.theme", null);
    expect(kept("muster.theme")).toBeNull();
  });

  it("is read once from the app, and each change handed to it at once", async () => {
    const app = {
      kept: vi.fn(async () => ({ "muster.theme": "system" })),
      keep: vi.fn(async () => {}),
    };
    await keepIn(app);
    expect(kept("muster.theme")).toBe("system");
    keep("muster.sessions.folds", "{}");
    keep("muster.theme", null);
    // Read from what the window holds, never asked of the app again.
    expect(kept("muster.sessions.folds")).toBe("{}");
    expect(kept("muster.theme")).toBeNull();
    expect(app.kept).toHaveBeenCalledTimes(1);
    expect(app.keep.mock.calls).toEqual([
      ["muster.sessions.folds", "{}"],
      ["muster.theme", null],
    ]);
    // Never in the page's storage, which writes to disk when it likes.
    expect(localStorage.length).toBe(0);
  });

  it("lasts as long as the window where the app cannot keep it", async () => {
    await keepIn({
      kept: async () => ({}),
      keep: async () => {
        throw new Error("disk full");
      },
    });
    keep("muster.theme", "light");
    expect(kept("muster.theme")).toBe("light");
  });
});
