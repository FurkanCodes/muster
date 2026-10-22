import { describe, expect, it } from "vitest";
import { Gpu, type GpuAddon, MOST_RETRIES } from "./gpu";

/** A pretend WebGL addon and the frames it is tried again on, as the test turns them. */
function rig(failLoads = 0) {
  const events: string[] = [];
  const addons: { lose: () => void; disposed: boolean }[] = [];
  let failing = failLoads;
  const pending = new Map<number, () => void>();
  let next = 1;
  const said: string[] = [];
  const gpu = new Gpu(
    () => {
      if (failing > 0) {
        failing -= 1;
        events.push("load failed");
        throw new Error("no context");
      }
      let listener: (() => void) | null = null;
      const addon = {
        disposed: false,
        lose: () => listener?.(),
      };
      const made: GpuAddon = {
        dispose: () => {
          addon.disposed = true;
          events.push("addon disposed");
        },
        onContextLoss: (heard) => {
          listener = heard;
          return { dispose: () => (listener = null) };
        },
      };
      addons.push(addon);
      events.push("loaded");
      return made;
    },
    () => [{ loseContext: () => events.push("context lost on purpose") }],
    {
      request: (callback) => {
        pending.set(next, callback);
        return next++;
      },
      cancel: (frame) => pending.delete(frame),
    },
    (words) => said.push(words),
  );
  const frame = () => {
    const due = [...pending.values()];
    pending.clear();
    for (const callback of due) callback();
  };
  return { gpu, events, addons, frame, pending, said, fail: (n: number) => (failing = n) };
}

describe("ARC-25 a terminal on the GPU while shown, and back on it after a lost context", () => {
  it("draws on the GPU while shown, and lets go of its context at once when given back", () => {
    const { gpu, events } = rig();
    gpu.want(true);
    expect(gpu.drawing).toBe(true);
    gpu.want(false);
    expect(gpu.drawing).toBe(false);
    // The addon first, then the context it held, lost on purpose.
    expect(events).toEqual(["loaded", "addon disposed", "context lost on purpose"]);
  });

  it("makes a lost context again on the next frame, not at once", () => {
    const { gpu, addons, frame } = rig();
    gpu.want(true);
    addons[0]?.lose();
    expect(gpu.drawing).toBe(false);
    expect(gpu.retrying).toBe(true);
    frame();
    expect(gpu.drawing).toBe(true);
    expect(addons).toHaveLength(2);
  });

  it("tries a few times a showing, then draws with the DOM and says so once", () => {
    const { gpu, addons, frame, pending, said } = rig();
    gpu.want(true);
    for (let loss = 0; loss <= MOST_RETRIES; loss += 1) {
      addons.at(-1)?.lose();
      frame();
    }
    expect(gpu.drawing).toBe(false);
    expect(pending.size).toBe(0);
    expect(said).toHaveLength(1);
    // The next showing tries afresh.
    gpu.want(false);
    gpu.want(true);
    expect(gpu.drawing).toBe(true);
  });

  it("tries again after a load that failed, and a showing after one that kept failing", () => {
    const { gpu, frame, fail } = rig(1);
    gpu.want(true);
    expect(gpu.drawing).toBe(false);
    frame();
    expect(gpu.drawing).toBe(true);

    gpu.want(false);
    fail(MOST_RETRIES + 1);
    gpu.want(true);
    for (let tries = 0; tries < MOST_RETRIES; tries += 1) frame();
    expect(gpu.drawing).toBe(false);
    expect(gpu.retrying).toBe(false);
    // Contexts to be had again: the next showing draws on the GPU.
    gpu.want(false);
    gpu.want(true);
    expect(gpu.drawing).toBe(true);
  });

  it("tries nothing while the window is not seen, and once it is seen again", () => {
    const { gpu, addons, pending } = rig();
    gpu.want(true);
    gpu.see(false);
    addons[0]?.lose();
    expect(pending.size).toBe(0);
    gpu.see(true);
    expect(gpu.drawing).toBe(true);
  });

  it("cancels a try due as it is given back, hidden or disposed", () => {
    for (const end of ["give back", "hide", "dispose"] as const) {
      const { gpu, addons, pending, frame } = rig();
      gpu.want(true);
      addons[0]?.lose();
      expect(pending.size).toBe(1);
      if (end === "give back") gpu.want(false);
      else if (end === "hide") gpu.see(false);
      else gpu.dispose();
      expect(pending.size).toBe(0);
      frame();
      expect(gpu.drawing).toBe(false);
    }
  });

  it("does not count a context its terminal let go of on purpose as lost", () => {
    const { gpu, addons, pending } = rig();
    gpu.want(true);
    const first = addons[0];
    gpu.want(false);
    first?.lose();
    expect(pending.size).toBe(0);
  });
});
