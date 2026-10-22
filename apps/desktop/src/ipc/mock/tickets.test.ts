import { describe, expect, it } from "vitest";
import type { BridgeFailure } from "../bridge";
import { MockBridge } from "./bridge";
import { ulid, workingDay } from "./scenario";

const muster = ulid(1);
const kepSite = ulid(3);
const kepApi = ulid(4);

function refusal(promise: Promise<unknown>): Promise<BridgeFailure> {
  return promise.then(
    () => {
      throw new Error("not refused");
    },
    (error: BridgeFailure) => error,
  );
}

describe("ARC-24 the mock daemon answers about tickets as supervisor/issues.rs does", () => {
  it("lists a project's tickets with where they are from, shared trackers saying so", async () => {
    const bridge = new MockBridge(workingDay());
    const reply = await bridge.command({ ListIssues: { project: muster } });
    if (typeof reply !== "object" || !("Issues" in reply)) throw new Error("no Issues");
    expect(reply.Issues.tracker).toBe("GitHub issues");
    expect(reply.Issues.source).toBeNull();
    expect(reply.Issues.issues[0]?.key).toBe("214");
    const site = await bridge.command({ ListIssues: { project: kepSite } });
    const api = await bridge.command({ ListIssues: { project: kepApi } });
    expect(site).toEqual(api);
    expect(await refusal(bridge.command({ ListIssues: { project: ulid(9) } }))).toMatchObject({
      code: "not_found",
    });
  });
  it("starts the first agent in the ticket's worktree and another in …-2", async () => {
    const bridge = new MockBridge(workingDay());
    const start = (another: boolean) =>
      bridge.command({
        StartFromIssue: {
          project: muster,
          key: "217",
          harness: "codex",
          size: { rows: 24, cols: 80 },
          another,
          on: null,
          brief: null,
        },
      });
    await start(false);
    await start(false);
    await start(true);
    const branches = bridge.world.workspaces
      .filter((w) => w.kind.branch.startsWith("muster/issue-217"))
      .map((w) => w.kind.branch);
    expect(branches).toEqual([
      "muster/issue-217-tickets-from-youtrack",
      "muster/issue-217-tickets-from-youtrack-2",
    ]);
    const titles = bridge.world.sessions.filter((s) => s.title.startsWith("#217 "));
    expect(titles).toHaveLength(3);
  });
  it("refuses to remove a worktree something runs in, and one with changes without force", async () => {
    const bridge = new MockBridge(workingDay());
    const busy = await refusal(
      bridge.command({ RemoveWorktree: { workspace: ulid(12), force: false } }),
    );
    expect(busy.message).toBe("stop the worktree's 2 running session(s) first");
    for (const s of bridge.world.sessions) if (s.workspace === ulid(12)) s.ended_at = 1;
    const dirty = await refusal(
      bridge.command({ RemoveWorktree: { workspace: ulid(12), force: false } }),
    );
    expect(dirty).toMatchObject({ code: "conflict" });
    expect(dirty.message).toContain("uncommitted changes");
    expect(await bridge.command({ RemoveWorktree: { workspace: ulid(12), force: true } })).toBe(
      "Done",
    );
    expect(bridge.world.workspaces.some((w) => w.id === ulid(12))).toBe(false);
  });
  it("closes a shared ticket in every listing of its tracker", async () => {
    const bridge = new MockBridge(workingDay());
    expect(await bridge.command({ CloseIssue: { project: kepSite, key: "KEP-31" } })).toBe("Done");
    expect(bridge.world.tickets[kepApi]?.issues.map((i) => i.key)).toEqual(["KEP-28"]);
  });
  it("puts the board's agents on their tickets when a test asks", () => {
    const bridge = new MockBridge(workingDay());
    bridge.agentsOnTickets();
    const branch = (id: string) => bridge.world.workspaces.find((w) => w.id === id)?.kind.branch;
    expect(branch(ulid(11))).toMatch(/^muster\/issue-214-/);
    expect(branch(ulid(12))).toMatch(/^muster\/issue-205-/);
    const gemini = bridge.world.sessions.find((s) => s.id === ulid(400));
    expect(branch(gemini?.workspace ?? "")).toMatch(/^muster\/issue-KEP-31-/);
  });
});
