import { describe, expect, it } from "vitest";
import { MockBridge } from "./bridge";
import { ulid, workingDay } from "./scenario";

describe("ARC-24 the mock daemon answers the queue's commands as the daemon does", () => {
  it("returns timelines newest first, an empty timeline with none kept, and typed refusals", async () => {
    const mock = new MockBridge(workingDay());
    expect(await mock.command({ Timeline: { workspace: ulid(11) } })).toEqual({
      Timeline: mock.world.timelines?.[ulid(11)],
    });
    expect(await mock.command({ Timeline: { workspace: ulid(10) } })).toEqual({
      Timeline: { entries: [] },
    });
    await expect(mock.command({ Timeline: { workspace: ulid(999) } })).rejects.toMatchObject({
      code: "not_found",
      message: `no workspace wsp_${ulid(999)}`,
    });
    await expect(
      mock.command({ EntryDiff: { workspace: ulid(10), number: 99 } }),
    ).rejects.toMatchObject({
      code: "not_found",
      message: "there is no entry 99 in this workspace's timeline",
    });
    const folder = mock.world.workspaces[0];
    if (folder) folder.kind.branch = "";
    await expect(mock.command({ Timeline: { workspace: ulid(10) } })).rejects.toMatchObject({
      code: "bad_request",
    });
  });
  it("renames on one line, refuses an empty or missing name, and caps it as the daemon does", async () => {
    const mock = new MockBridge(workingDay());
    await expect(
      mock.command({ RenameSession: { session: ulid(101), title: "  A\n name  " } }),
    ).resolves.toBe("Done");
    expect(mock.world.sessions.find((s) => s.id === ulid(101))?.title).toBe("A name");
    await expect(
      mock.command({ RenameSession: { session: ulid(101), title: " " } }),
    ).rejects.toMatchObject({ code: "bad_request" });
    await expect(
      mock.command({ RenameSession: { session: ulid(999), title: "Gone" } }),
    ).rejects.toMatchObject({ code: "not_found" });
    await mock.command({ RenameSession: { session: ulid(101), title: "a".repeat(100) } });
    expect(mock.world.sessions.find((s) => s.id === ulid(101))?.title).toHaveLength(80);
  });
  it("keeps a running session until stopped and refuses missing or already exited sessions", async () => {
    const mock = new MockBridge(workingDay());
    await expect(mock.command({ RemoveSession: { session: ulid(101) } })).rejects.toMatchObject({
      code: "conflict",
    });
    await mock.command({ StopSession: { session: ulid(101) } });
    await Promise.resolve();
    // Its program's exit is told after the answer, with when it ended.
    expect(mock.world.sessions.find((s) => s.id === ulid(101))).toMatchObject({
      status: { state: "exited", success: true },
      ended_at: expect.any(Number),
    });
    await expect(mock.command({ StopSession: { session: ulid(101) } })).rejects.toMatchObject({
      code: "conflict",
    });
    await expect(mock.command({ RemoveSession: { session: ulid(101) } })).resolves.toBe("Done");
    await expect(mock.command({ RemoveSession: { session: ulid(101) } })).rejects.toMatchObject({
      code: "not_found",
    });
    await expect(mock.command({ StopSession: { session: ulid(101) } })).rejects.toMatchObject({
      code: "not_found",
    });
  });
  it("ARC-25 clears news only while the session is attached and looked at", async () => {
    const mock = new MockBridge(workingDay());
    const id = ulid(104);
    const stored = () => mock.world.sessions.find((s) => s.id === id);
    await mock.attach(id, null, true, () => {});
    expect(stored()?.unseen).toBe(true);
    await mock.looking(true, id);
    expect(stored()?.unseen).toBe(false);
    await mock.detach(id);
    await mock.looking(false, null);
    mock.setStatus(id, { state: "finished" }, true);
    await mock.watch(id, true);
    expect(stored()?.unseen).toBe(true);
  });
});
