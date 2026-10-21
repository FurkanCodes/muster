import { describe, expect, it } from "vitest";
import { derive } from "../../ipc/mock/model";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { createDaemon, type DaemonState } from "../../state/daemon";
import { holdPlace, queueIds, sectionOf } from "./queue";

function stateOf(world = workingDay()): DaemonState {
  return {
    ...createDaemon().store.getState(),
    loaded: true,
    projects: Object.fromEntries(world.projects.map((x) => [x.id, x])),
    workspaces: Object.fromEntries(world.workspaces.map((x) => [x.id, x])),
    sessions: Object.fromEntries(world.sessions.map((x) => [x.id, x])),
    derived: derive(world, new Map()),
  };
}

describe("ARC-24 the queue's lists, in the model's order", () => {
  it("lists Needs you as the model orders it, then the agents at work, the longest first", () => {
    const state = stateOf();
    expect(state.derived.needs_you).toEqual([ulid(101), ulid(200), ulid(104), ulid(202)]);
    expect(queueIds(state, null)).toEqual([
      ulid(101),
      ulid(200),
      ulid(104),
      ulid(202),
      ulid(201),
      ulid(100),
      ulid(103),
      ulid(400),
    ]);
  });

  it("never orders the queue itself, whatever the times say", () => {
    const state = stateOf();
    const reversed = {
      ...state,
      derived: { ...state.derived, needs_you: [ulid(202), ulid(104), ulid(200), ulid(101)] },
    };
    expect(queueIds(reversed, null).slice(0, 4)).toEqual(reversed.derived.needs_you);
    const working = [ulid(400), ulid(100), ulid(201), ulid(103)];
    expect(queueIds({ ...state, derived: { ...state.derived, working } }, null).slice(4)).toEqual(
      working,
    );
  });

  it("lists the agents at work after those waiting, never a shell", () => {
    const state = stateOf();
    expect(queueIds(state, null).slice(4)).toEqual([ulid(201), ulid(100), ulid(103), ulid(400)]);
    expect(queueIds(state, null)).not.toContain(ulid(102));
  });

  it("narrows the list to one project", () => {
    const state = stateOf();
    expect(queueIds(state, ulid(2))).toEqual([ulid(200), ulid(202), ulid(201)]);
    expect(queueIds(state, ulid(3))).toEqual([]);
  });

  it("puts each session in its section by how it stands", () => {
    const sections = Object.values(stateOf().sessions).map((session) => [
      session.title,
      sectionOf(session),
    ]);
    expect(sections).toContainEqual(["Sketch the Tauri shell", "asking"]);
    expect(sections).toContainEqual(["Resize settles once on release", "news"]);
    expect(sections).toContainEqual(["Upgrade AGP to 9.1", "working"]);
  });
});

describe("ARC-24 selection holds its place when an item leaves", () => {
  const shown = [ulid(101), ulid(104), ulid(202)];

  it("keeps the selected one where it was, once seen, until another is selected", () => {
    expect(holdPlace([ulid(101), ulid(202)], shown, ulid(104), true)).toEqual({
      ids: shown,
      selected: ulid(104),
    });
  });

  it("selects the one now at its place when it leaves for good", () => {
    expect(holdPlace([ulid(101), ulid(202)], shown, ulid(104), false)).toEqual({
      ids: [ulid(101), ulid(202)],
      selected: ulid(202),
    });
    expect(holdPlace([ulid(101)], shown, ulid(202), false).selected).toBe(ulid(101));
    expect(holdPlace([], shown, ulid(104), false).selected).toBeUndefined();
  });

  it("selects the first where nothing was selected, and keeps a selection still there", () => {
    expect(holdPlace(shown, [], undefined, false).selected).toBe(ulid(101));
    expect(holdPlace(shown, shown, ulid(202), false)).toEqual({ ids: shown, selected: ulid(202) });
  });
});
