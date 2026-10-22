import { describe, expect, it, vi } from "vitest";
import type { Bridge } from "../ipc/bridge";
import { KeptTerminal, loadWebgl } from "./kept";

const bridge = {
  input: vi.fn(() => Promise.resolve()),
} as unknown as Bridge;

const output = (seq: number, text: string) => ({
  kind: "output" as const,
  seq,
  data: new TextEncoder().encode(text),
});

describe("ARC-25 a session's kept terminal", () => {
  it("takes frames only from its own stream, and none once disposed", () => {
    const kept = new KeptTerminal(bridge, "s", false);
    const write = vi.spyOn(kept.xterm, "write");
    const old = kept.startFeed();
    const now = kept.startFeed();
    old.onFrame(output(0, "late"));
    expect(write).not.toHaveBeenCalled();
    expect(kept.nextSeq).toBeNull();
    now.onFrame(output(0, "new"));
    expect(write).toHaveBeenCalledTimes(1);
    expect(kept.nextSeq).toBe(3);
    // A stream let go of by a showing gone takes no frame either.
    kept.stopFeed(now);
    now.onFrame(output(3, "after"));
    expect(write).toHaveBeenCalledTimes(1);
    const last = kept.startFeed();
    kept.dispose();
    last.onFrame(output(3, "gone"));
    expect(write).toHaveBeenCalledTimes(1);
    expect(kept.isDisposed).toBe(true);
  });

  it("lets only its own stream be stopped by a showing that goes", () => {
    const kept = new KeptTerminal(bridge, "s", false);
    const old = kept.startFeed();
    const now = kept.startFeed();
    kept.stopFeed(old);
    const write = vi.spyOn(kept.xterm, "write");
    now.onFrame(output(0, "still"));
    expect(write).toHaveBeenCalledTimes(1);
    kept.dispose();
  });

  it("changes its size in place, never made again for it", () => {
    const kept = new KeptTerminal(bridge, "s", false);
    const xterm = kept.xterm;
    kept.setCompact(true);
    expect(kept.xterm).toBe(xterm);
    expect(xterm.options.fontSize).toBe(12);
    kept.setCompact(false);
    expect(xterm.options.fontSize).toBe(13);
    kept.dispose();
  });
});

describe("ARC-25 a WebGL addon that cannot start", () => {
  it("is disposed, so it leaves xterm's list, and the failure goes on", () => {
    const listed: unknown[] = [];
    const addon = { dispose: vi.fn(() => listed.splice(listed.indexOf(addon), 1)) };
    const xterm = {
      loadAddon: (loaded: typeof addon) => {
        // As xterm's AddonManager: listed first, then started.
        listed.push(loaded);
        throw new Error("WebGL2 not supported");
      },
    };
    expect(() => loadWebgl(xterm, () => addon)).toThrow("WebGL2 not supported");
    expect(addon.dispose).toHaveBeenCalledTimes(1);
    expect(listed).toEqual([]);
  });

  it("that could not even be made disposes nothing", () => {
    const xterm = { loadAddon: vi.fn() };
    expect(() =>
      loadWebgl(xterm, () => {
        throw new Error("no addon");
      }),
    ).toThrow("no addon");
    expect(xterm.loadAddon).not.toHaveBeenCalled();
  });

  it("started is given back as it is", () => {
    const addon = { dispose: vi.fn() };
    const xterm = { loadAddon: vi.fn() };
    expect(loadWebgl(xterm, () => addon)).toBe(addon);
    expect(addon.dispose).not.toHaveBeenCalled();
  });
});
