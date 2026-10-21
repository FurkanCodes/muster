import { describe, expect, it } from "vitest";
import type { Patch } from "../ipc/generated/desktop/Patch";
import { workingDay } from "../ipc/mock/scenario";
import { createDaemon, emptyDerived } from "./daemon";

function patch(parts: Partial<Patch>): Patch {
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
    derived: emptyDerived(),
    ...parts,
  };
}

describe("the daemon's state in the webview", () => {
  const world = workingDay();

  it("is replaced whole by a reset patch", () => {
    const daemon = createDaemon();
    daemon.apply({ kind: "patch", patch: patch({ reset: true, sessions: world.sessions }) });
    expect(Object.keys(daemon.store.getState().sessions)).toHaveLength(world.sessions.length);
    expect(daemon.store.getState().loaded).toBe(true);
  });

  it("keeps every session a patch does not name as the same object", () => {
    const daemon = createDaemon();
    daemon.apply({ kind: "patch", patch: patch({ reset: true, sessions: world.sessions }) });
    const [first, second] = world.sessions;
    if (!first || !second) throw new Error("the scenario has sessions");
    const before = daemon.store.getState().sessions;
    daemon.apply({ kind: "patch", patch: patch({ sessions: [{ ...first, title: "Renamed" }] }) });
    const after = daemon.store.getState().sessions;
    expect(after[first.id]?.title).toBe("Renamed");
    expect(after[second.id]).toBe(before[second.id]);
  });

  it("takes out what went", () => {
    const daemon = createDaemon();
    daemon.apply({ kind: "patch", patch: patch({ reset: true, sessions: world.sessions }) });
    const gone = world.sessions[0]?.id ?? "";
    daemon.apply({
      kind: "patch",
      patch: patch({ gone: { projects: [], workspaces: [], sessions: [gone] } }),
    });
    expect(daemon.store.getState().sessions[gone]).toBeUndefined();
  });

  it("keeps derived parts that did not change, so their readers do not re-render", () => {
    const daemon = createDaemon();
    const derived = { needs_you: ["a"], rows: [], counts: { working: 1, waiting: 1, done: 0 } };
    daemon.apply({ kind: "patch", patch: patch({ reset: true, derived }) });
    const held = daemon.store.getState().derived;
    daemon.apply({ kind: "patch", patch: patch({ derived: { ...derived, needs_you: ["a"] } }) });
    expect(daemon.store.getState().derived).toBe(held);
    daemon.apply({
      kind: "patch",
      patch: patch({ derived: { ...derived, needs_you: ["a", "b"] } }),
    });
    expect(daemon.store.getState().derived.counts).toBe(held.counts);
    expect(daemon.store.getState().derived.needs_you).toEqual(["a", "b"]);
  });

  it("tells the reveal listeners", () => {
    const daemon = createDaemon();
    const told: string[] = [];
    daemon.onReveal((session) => told.push(session));
    daemon.apply({ kind: "reveal", session: "s1" });
    expect(told).toEqual(["s1"]);
  });
});
