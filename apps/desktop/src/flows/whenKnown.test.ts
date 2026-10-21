import { describe, expect, it, vi } from "vitest";
import { MockBridge } from "../ipc/mock/bridge";
import { workingDay } from "../ipc/mock/scenario";
import { renderWithApp } from "../test/render";
import { whenKnown } from "./whenKnown";

describe("ARC-24 what a command made is acted on once the window holds it", () => {
  it("waits for a session the daemon told after its answer, and acts at once on one it holds", async () => {
    const bridge = new MockBridge(workingDay());
    const { app } = renderWithApp(null, bridge);
    await vi.waitFor(() => expect(app.daemon.store.getState().loaded).toBe(true));
    const known = Object.keys(app.daemon.store.getState().sessions)[0] ?? "";
    const now = vi.fn();
    whenKnown(app, "sessions", known, now);
    expect(now).toHaveBeenCalledOnce();

    const later = vi.fn();
    const fresh = "01JD0000000000000000009000";
    whenKnown(app, "sessions", fresh, later);
    expect(later).not.toHaveBeenCalled();
    const like = bridge.world.sessions[0];
    if (!like) throw new Error("no session");
    bridge.world.sessions.push({ ...like, id: fresh });
    bridge.emitSession(fresh);
    expect(later).toHaveBeenCalledOnce();
    bridge.emitSession(fresh);
    expect(later).toHaveBeenCalledOnce();
  });
});
