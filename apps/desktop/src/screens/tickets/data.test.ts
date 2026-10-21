import { describe, expect, it, vi } from "vitest";
import type { Reply } from "../../ipc/generated/Reply";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { groupProjects, ticketId, ticketsFor } from "./data";

describe("ARC-24 ARC-25 tickets use daemon replies without polling", () => {
  it("fetches once per project and groups only identical non-null sources", async () => {
    const bridge = new MockBridge(workingDay());
    const model = ticketsFor(bridge);
    const ids = bridge.world.projects.map((p) => p.id);
    await Promise.all(ids.map((id) => model.fetch(id)));
    await model.fetch(ulid(1));
    expect(bridge.sent).toHaveLength(4);
    expect(groupProjects(ids, model.store.getState().lists)).toEqual([
      [ids[0]],
      [ids[1]],
      [ids[2], ids[3]],
    ]);
    expect(ticketsFor(bridge)).toBe(model);
  });
  it("keeps a refusal until refresh, and reads settings back before fetching again", async () => {
    const bridge = new MockBridge(workingDay());
    const model = ticketsFor(bridge);
    bridge.failNext("ListIssues", "gh is not signed in");
    await model.fetch(ulid(1));
    expect(model.store.getState().lists[ulid(1)]).toEqual({
      state: "failed",
      error: "gh is not signed in",
    });
    await model.fetch(ulid(1));
    expect(bridge.sent).toHaveLength(1);
    model.forgetFailed();
    await model.fetch(ulid(1));
    await model.mine([ulid(1)], true);
    expect(bridge.sent.slice(-2)).toEqual([
      { ChangeSetting: { project: ulid(1), change: { OnlyMyTickets: true } } },
      { ListIssues: { project: ulid(1) } },
    ]);
    expect(bridge.world.tickets[ulid(1)]?.mine).toBe(true);
  });
  it("PRD-01 changes Only my tickets for every project sharing them, so they stay one group", async () => {
    const bridge = new MockBridge(workingDay());
    const model = ticketsFor(bridge);
    const ids = bridge.world.projects.map((p) => p.id);
    await Promise.all(ids.map((id) => model.fetch(id)));
    await model.mine([ulid(3), ulid(4)], true);
    expect(bridge.sent.filter((c) => typeof c !== "string" && "ChangeSetting" in c)).toEqual([
      { ChangeSetting: { project: ulid(3), change: { OnlyMyTickets: true } } },
      { ChangeSetting: { project: ulid(4), change: { OnlyMyTickets: true } } },
    ]);
    expect(groupProjects(ids, model.store.getState().lists).at(-1)).toEqual([ulid(3), ulid(4)]);
    expect(model.store.getState().changing[ulid(3)]).toBe(false);
  });
  it("loads body once and removes a closed ticket from every shared listing", async () => {
    const bridge = new MockBridge(workingDay());
    const model = ticketsFor(bridge);
    const issue = bridge.world.tickets[ulid(3)]?.issues[0];
    if (!issue) throw new Error("missing ticket fixture");
    await model.view(ulid(3), issue);
    await model.view(ulid(3), issue);
    expect(model.store.getState().texts[ticketId(ulid(3), issue.key)]?.state).toBe("loaded");
    expect(bridge.sent).toHaveLength(1);
    await Promise.all([model.fetch(ulid(3)), model.fetch(ulid(4))]);
    model.closed([ulid(3), ulid(4)], issue.key);
    for (const id of [ulid(3), ulid(4)]) {
      const list = model.store.getState().lists[id];
      expect(list?.state === "loaded" && list.value.issues.some((i) => i.key === issue.key)).toBe(
        false,
      );
    }
  });
  it("ARC-14 drops removed projects and ignores replies superseded by refresh", async () => {
    const bridge = new MockBridge(workingDay());
    const model = ticketsFor(bridge);
    let answer: (reply: Reply) => void = () => {};
    vi.spyOn(bridge, "command").mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          answer = resolve;
        }),
    );
    const old = model.fetch(ulid(1));
    await model.fetch(ulid(1), true);
    answer({ Issues: { issues: [], tracker: "old", source: null, mine: false } });
    await old;
    const list = model.store.getState().lists[ulid(1)];
    expect(list?.state === "loaded" && list.value.tracker).toBe("GitHub issues");
    model.prune([]);
    expect(model.store.getState().lists).toEqual({});
  });
});
