// The All sessions tree's rows, folds, folded marks and narrowing, as
// `app::fold` and `Model::most_urgent` in the terminal UI (ARC-24, PRD §6).

import { beforeEach, describe, expect, it } from "vitest";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { createDaemon, type DaemonState } from "../../state/daemon";
import { readFolds, writeFolds } from "./folds";
import {
  allRows,
  foldable,
  keptFolds,
  mostUrgent,
  selectionAfterFold,
  sessionsOn,
  shownRows,
  toggled,
  underItsChat,
  unfoldedTo,
} from "./tree";

const muster = ulid(1);
const lamphaus = ulid(2);
const kepSite = ulid(3);
const checkout = ulid(10);
const desktop = ulid(11);
const resize = ulid(12);
const kepSiteMain = ulid(30);

async function stateOf(bridge = new MockBridge(workingDay())): Promise<DaemonState> {
  const daemon = createDaemon();
  await bridge.open((message) => daemon.apply(message));
  await Promise.resolve();
  return daemon.store.getState();
}

describe("the tree's rows", () => {
  it("are the model's rows as they come, each with its project and workspace", async () => {
    const state = await stateOf();
    const rows = allRows(state, null);
    expect(rows.map((row) => row.id)).toEqual(state.derived.rows.map((row) => row.id));
    const shell = rows.find((row) => row.id === ulid(102));
    expect(shell).toEqual({ kind: "session", id: ulid(102), project: muster, workspace: desktop });
    expect(rows[0]).toEqual({ kind: "project", id: muster, project: muster, workspace: null });
  });

  it("narrow to the project the window is narrowed to", async () => {
    const state = await stateOf();
    const rows = allRows(state, lamphaus);
    expect(new Set(rows.map((row) => row.project))).toEqual(new Set([lamphaus]));
    expect(rows[0]?.id).toBe(lamphaus);
  });

  it("leave out what a folded project or worktree hides, but the folded line itself", async () => {
    const state = await stateOf();
    const rows = allRows(state, null);
    const shown = shownRows(rows, new Set([desktop, lamphaus]));
    const ids = shown.map((row) => row.id);
    expect(ids).toContain(desktop);
    expect(ids).not.toContain(ulid(101));
    expect(ids).not.toContain(ulid(102));
    expect(ids).toContain(lamphaus);
    expect(ids).not.toContain(ulid(20));
    expect(ids).toContain(kepSite);
  });
});

describe("folding", () => {
  it("folds a project always and a worktree only once a session is in it", async () => {
    const state = await stateOf();
    expect(foldable(state, { kind: "project", id: kepSite })).toBe(true);
    expect(foldable(state, { kind: "workspace", id: kepSiteMain })).toBe(false);
    expect(foldable(state, { kind: "workspace", id: desktop })).toBe(true);
    expect(foldable(state, { kind: "session", id: ulid(101) })).toBe(false);
  });

  it("toggles, and refuses a line with nothing to fold away", async () => {
    const state = await stateOf();
    const row = allRows(state, null).find((one) => one.id === kepSiteMain);
    if (!row) throw new Error("no kep-site checkout");
    expect(toggled(new Set(), row, false)).toEqual({ folded: new Set(), refused: true });
    const desktopRow = allRows(state, null).find((one) => one.id === desktop);
    if (!desktopRow) throw new Error("no desktop worktree");
    const once = toggled(new Set(), desktopRow, true);
    expect(once.folded.has(desktop)).toBe(true);
    expect(toggled(once.folded, desktopRow, true).folded.has(desktop)).toBe(false);
  });

  it("takes the selection to the line folded when it hid it", async () => {
    const rows = allRows(await stateOf(), null);
    const find = (id: string) => rows.find((row) => row.id === id);
    const codex = find(ulid(101));
    const project = find(muster);
    const worktree = find(desktop);
    if (!codex || !project || !worktree) throw new Error("missing rows");
    expect(selectionAfterFold(worktree, codex)).toBe(desktop);
    expect(selectionAfterFold(project, codex)).toBe(muster);
    expect(selectionAfterFold(project, worktree)).toBe(muster);
    expect(selectionAfterFold(find(resize) ?? worktree, codex)).toBe(ulid(101));
  });

  it("unfolds what hides a line selected for the person", async () => {
    const rows = allRows(await stateOf(), null);
    const codex = rows.find((row) => row.id === ulid(101));
    if (!codex) throw new Error("no codex");
    const folds = new Set([muster, desktop, lamphaus]);
    expect(unfoldedTo(folds, codex)).toEqual(new Set([lamphaus]));
    const none = new Set([lamphaus]);
    expect(unfoldedTo(none, codex)).toBe(none);
  });

  it("forgets folds of lines gone or with nothing left to hide", async () => {
    const state = await stateOf();
    const kept = new Set([muster, desktop]);
    expect(keptFolds(state, kept)).toBe(kept);
    expect(keptFolds(state, new Set([muster, kepSiteMain, "gone"]))).toEqual(new Set([muster]));
  });
});

describe("a folded line's mark", () => {
  it("is the most urgent of what it hides: asking first, then news, then working", async () => {
    const bridge = new MockBridge(workingDay());
    let state = await stateOf(bridge);
    // muster: Codex asks, two work, one finished unseen.
    expect(mostUrgent(state, { kind: "project", id: muster })?.id).toBe(ulid(101));
    // fix/resize: one works, one finished unseen: the news first.
    expect(mostUrgent(state, { kind: "workspace", id: resize })?.id).toBe(ulid(104));
    // main: one works.
    expect(mostUrgent(state, { kind: "workspace", id: checkout })?.id).toBe(ulid(100));
    expect(mostUrgent(state, { kind: "workspace", id: kepSiteMain })).toBeNull();
    bridge.setStatus(ulid(104), { state: "finished" }, false);
    state = await stateOf(bridge);
    expect(mostUrgent(state, { kind: "workspace", id: resize })?.id).toBe(ulid(103));
  });

  it("counts every session it hides, shells too, as the terminal UI does", async () => {
    const state = await stateOf();
    expect(sessionsOn(state, { kind: "workspace", id: desktop })).toHaveLength(2);
    expect(sessionsOn(state, { kind: "project", id: muster })).toHaveLength(5);
  });
});

describe("a shell's line", () => {
  it("stands under its worktree's one chat, and after the chats where several share it", async () => {
    const bridge = new MockBridge(workingDay());
    let state = await stateOf(bridge);
    const shell = state.sessions[ulid(102)];
    if (!shell) throw new Error("no shell");
    expect(underItsChat(state, shell)).toBe(true);
    bridge.world.sessions.push({ ...shell, id: ulid(105), harness: "claude", title: "Another" });
    state = await stateOf(bridge);
    expect(underItsChat(state, shell)).toBe(false);
    const chat = state.sessions[ulid(101)];
    if (!chat) throw new Error("no chat");
    expect(underItsChat(state, chat)).toBe(false);
  });
});

describe("the folds kept", () => {
  beforeEach(() => localStorage.clear());

  it("are kept per project across restarts", async () => {
    const state = await stateOf();
    writeFolds(state, new Set([muster, desktop, lamphaus]));
    const kept = JSON.parse(localStorage.getItem("muster.sessions.folds") ?? "{}");
    expect(kept).toEqual({
      version: 1,
      projects: { [muster]: [muster, desktop], [lamphaus]: [lamphaus] },
    });
    expect(readFolds()).toEqual(new Set([muster, desktop, lamphaus]));
  });

  it("are none where storage holds something else", () => {
    localStorage.setItem("muster.sessions.folds", "not json");
    expect(readFolds()).toEqual(new Set());
    localStorage.setItem("muster.sessions.folds", JSON.stringify({ version: 2 }));
    expect(readFolds()).toEqual(new Set());
  });
});
