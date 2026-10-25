// The All list's rows, folds and folded marks, as `app::fold` and
// `Model::most_urgent` in the terminal UI (ARC-24, PRD §6a): projects and
// their sessions, a worktree said over its session rather than on a line.

import { beforeEach, describe, expect, it } from "vitest";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { createDaemon, type DaemonState } from "../../state/daemon";
import { readFolds, writeFolds } from "./folds";
import {
  allRows,
  keptFolds,
  mostUrgent,
  selectionAfterFold,
  sessionsOn,
  shownRows,
  toggled,
  unfoldedTo,
} from "./tree";

const muster = ulid(1);
const lamphaus = ulid(2);
const kepSite = ulid(3);
const desktop = ulid(11);
const kepSiteMain = ulid(30);

async function stateOf(bridge = new MockBridge(workingDay())): Promise<DaemonState> {
  const daemon = createDaemon();
  await bridge.open((message) => daemon.apply(message));
  await Promise.resolve();
  return daemon.store.getState();
}

describe("ARC-33 the All list under each space's heading", () => {
  it("lists each space's heading, what it holds under it, and folds it all away", async () => {
    const bridge = new MockBridge(workingDay());
    await bridge.command({ CreateSpace: { name: "Work" } });
    await bridge.command({ CreateSpace: { name: "Home" } });
    const [work, home] = (bridge.world.spaces ?? []).map((space) => space.id);
    if (!work || !home) throw new Error("no spaces");
    await bridge.command({ MoveToSpace: { space: work, member: { Project: lamphaus } } });
    await bridge.command({ MoveToSpace: { space: home, member: { Project: muster } } });
    const state = await stateOf(bridge);
    const rows = allRows(state);
    expect(rows[0]).toEqual({ kind: "space", id: work, project: "", space: work, workspace: null });
    expect(rows[1]).toMatchObject({ kind: "project", id: lamphaus, space: work });
    const homeAt = rows.findIndex((row) => row.id === home);
    expect(rows[homeAt + 1]).toMatchObject({ kind: "project", id: muster, space: home });
    // In no space yet, as between deltas: last, with no heading.
    expect(rows.at(-1)?.space).toBe("");
    // Folding Work leaves its heading and every other line.
    const folded = shownRows(rows, toggled(new Set(), work));
    expect(folded.some((row) => row.space === work && row.kind !== "space")).toBe(false);
    expect(folded.map((row) => row.id)).toContain(muster);
    expect(selectionAfterFold(work, rows[1])).toBe(work);
    expect(unfoldedTo(new Set([work]), rows[1] as never).has(work)).toBe(false);
    // A space stands for its projects' sessions.
    expect(sessionsOn(state, { kind: "space", id: home })).toEqual(
      sessionsOn(state, { kind: "project", id: muster }),
    );
  });
});

describe("ARC-24 the All list's rows", () => {
  it("are the projects and their sessions in the model's order, no worktree's line among them", async () => {
    const state = await stateOf();
    const rows = allRows(state);
    expect(rows.some((row) => (row.kind as string) === "workspace")).toBe(false);
    expect(rows[0]).toEqual({
      kind: "project",
      id: muster,
      project: muster,
      space: "",
      workspace: null,
    });
    const sessions = state.derived.rows.filter((row) => row.kind !== "workspace");
    expect(rows.map((row) => row.id)).toEqual(
      sessions.flatMap((row) => ("id" in row ? [row.id] : [])).filter((id) => id !== ulid(102)),
    );
    expect(rows.find((row) => row.id === ulid(101))).toEqual({
      kind: "session",
      id: ulid(101),
      project: muster,
      space: "",
      workspace: desktop,
    });
  });

  it("leave out a shell its workspace's chat has beside it, and list one with no chat to stand by", async () => {
    const bridge = new MockBridge(workingDay());
    expect(allRows(await stateOf(bridge)).map((row) => row.id)).not.toContain(ulid(102));
    // Its chat closed, the shell is the workspace's only session: it has a line of its own.
    bridge.world.sessions = bridge.world.sessions.filter((session) => session.id !== ulid(101));
    const rows = allRows(await stateOf(bridge));
    expect(rows.find((row) => row.id === ulid(102))).toEqual({
      kind: "session",
      id: ulid(102),
      project: muster,
      space: "",
      workspace: desktop,
    });
  });

  it("give a worktree no session is in a line of its own, to start one there or remove it", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.sessions = bridge.world.sessions.filter((session) => session.id !== ulid(201));
    const rows = allRows(await stateOf(bridge));
    expect(rows.find((row) => row.id === ulid(22))).toEqual({
      kind: "workspace",
      id: ulid(22),
      project: lamphaus,
      space: "",
      workspace: ulid(22),
    });
    // A project's own checkout never: its line is the project's.
    expect(rows.find((row) => row.id === kepSiteMain)).toBeUndefined();
    const folded = shownRows(rows, new Set([lamphaus])).map((row) => row.id);
    expect(folded).not.toContain(ulid(22));
  });

  it("leave out what a folded project hides, but the project's line itself", async () => {
    const rows = allRows(await stateOf());
    const ids = shownRows(rows, new Set([lamphaus])).map((row) => row.id);
    expect(ids).toContain(lamphaus);
    expect(ids).not.toContain(ulid(200));
    expect(ids).toContain(ulid(101));
    expect(ids).toContain(kepSite);
  });
});

describe("ARC-24 folding", () => {
  it("toggles a project's line", () => {
    const once = toggled(new Set(), muster);
    expect(once).toEqual(new Set([muster]));
    expect(toggled(once, muster)).toEqual(new Set());
  });

  it("takes the selection to the project folded when it hid it", async () => {
    const rows = allRows(await stateOf());
    const codex = rows.find((row) => row.id === ulid(101));
    const project = rows.find((row) => row.id === muster);
    if (!codex || !project) throw new Error("missing rows");
    expect(selectionAfterFold(muster, codex)).toBe(muster);
    expect(selectionAfterFold(muster, project)).toBe(muster);
    expect(selectionAfterFold(lamphaus, codex)).toBe(ulid(101));
  });

  it("unfolds the project that hides a line selected for the person", async () => {
    const rows = allRows(await stateOf());
    const codex = rows.find((row) => row.id === ulid(101));
    if (!codex) throw new Error("no codex");
    expect(unfoldedTo(new Set([muster, lamphaus]), codex)).toEqual(new Set([lamphaus]));
    const none = new Set([lamphaus]);
    expect(unfoldedTo(none, codex)).toBe(none);
  });

  it("forgets the folds of projects gone, and a worktree's kept from before", async () => {
    const state = await stateOf();
    const kept = new Set([muster, lamphaus]);
    expect(keptFolds(state, kept)).toBe(kept);
    expect(keptFolds(state, new Set([muster, desktop, kepSiteMain, "gone"]))).toEqual(
      new Set([muster]),
    );
  });
});

describe("ARC-24 a folded project's mark", () => {
  it("is the most urgent of what it hides: asking first, then news, then working", async () => {
    const bridge = new MockBridge(workingDay());
    let state = await stateOf(bridge);
    // muster: Codex asks, two work, one finished unseen.
    expect(mostUrgent(state, { kind: "project", id: muster })?.id).toBe(ulid(101));
    expect(mostUrgent(state, { kind: "project", id: kepSite })).toBeNull();
    bridge.setStatus(ulid(101), { state: "running" }, false);
    state = await stateOf(bridge);
    // Then the news not seen before the working.
    expect(mostUrgent(state, { kind: "project", id: muster })?.id).toBe(ulid(104));
  });

  it("counts every session it hides, shells too, as the terminal UI does", async () => {
    const state = await stateOf();
    expect(sessionsOn(state, { kind: "project", id: muster })).toHaveLength(5);
    expect(sessionsOn(state, { kind: "session", id: ulid(101) })).toHaveLength(1);
  });
});

describe("ARC-24 the folds kept", () => {
  beforeEach(() => localStorage.clear());

  it("are kept per project across restarts", async () => {
    const state = await stateOf();
    writeFolds(state, new Set([muster, desktop, lamphaus]));
    const kept = JSON.parse(localStorage.getItem("muster.sessions.folds") ?? "{}");
    expect(kept).toEqual({ version: 1, projects: { [muster]: [muster], [lamphaus]: [lamphaus] } });
    expect(readFolds()).toEqual(new Set([muster, lamphaus]));
  });

  it("are none where storage holds something else", () => {
    localStorage.setItem("muster.sessions.folds", "not json");
    expect(readFolds()).toEqual(new Set());
    localStorage.setItem("muster.sessions.folds", JSON.stringify({ version: 2 }));
    expect(readFolds()).toEqual(new Set());
  });
});
