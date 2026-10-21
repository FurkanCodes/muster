import { describe, expect, it } from "vitest";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { createDaemon } from "../../state/daemon";
import { changedInSight, installQueueHistory, lastNeeded } from "./history";

async function watched() {
  const daemon = createDaemon();
  const stop = installQueueHistory(daemon);
  const mock = new MockBridge(workingDay());
  await mock.open((message) => daemon.apply(message));
  await Promise.resolve();
  const session = (id: string) => {
    const found = daemon.store.getState().sessions[id];
    if (!found) throw new Error(`no session ${id}`);
    return found;
  };
  return { daemon, mock, stop, session };
}

describe("ARC-24 the session that needed the person last (l)", () => {
  it("is none of those already waiting as the window opens", async () => {
    const { daemon, stop } = await watched();
    expect(lastNeeded(daemon)).toBeUndefined();
    stop();
  });

  it("is the one that came to ask or finish last, kept once it is dealt with", async () => {
    const { daemon, mock, stop } = await watched();
    mock.setStatus(ulid(100), { state: "needs_you", reason: "question" });
    expect(lastNeeded(daemon)).toBe(ulid(100));
    mock.setStatus(ulid(103), { state: "finished" }, true);
    expect(lastNeeded(daemon)).toBe(ulid(103));
    mock.setStatus(ulid(103), { state: "running" });
    expect(lastNeeded(daemon)).toBe(ulid(103));
    // Its news seen is no new need.
    mock.updateSession(ulid(104), { unseen: false });
    mock.emitSession(ulid(104));
    expect(lastNeeded(daemon)).toBe(ulid(103));
    // The same session asking again is.
    mock.setStatus(ulid(100), { state: "needs_you", reason: "permission" });
    expect(lastNeeded(daemon)).toBe(ulid(100));
    stop();
  });

  it("is forgotten as its session goes", async () => {
    const { daemon, mock, stop } = await watched();
    mock.setStatus(ulid(100), { state: "needs_you", reason: "question" });
    mock.removeSession(ulid(100));
    expect(lastNeeded(daemon)).toBeUndefined();
    stop();
  });

  it("takes nothing the snapshot brings after the link came back as news", async () => {
    const { daemon, mock, stop, session } = await watched();
    mock.setLink({ state: "down", reason: "the daemon stopped", since: 0 });
    mock.setLink({ state: "connecting" });
    mock.updateSession(ulid(103), { status: { state: "needs_you", reason: "question" } });
    await mock.open((message) => daemon.apply(message));
    await Promise.resolve();
    expect(session(ulid(103)).status.state).toBe("needs_you");
    expect(lastNeeded(daemon)).toBeUndefined();
    expect(changedInSight(daemon, session(ulid(103)))).toBe(false);
    stop();
  });
});

describe("ARC-24 rows whose status changed in sight flash once", () => {
  it("flashes a change watched, not what was there, nor a change since replaced", async () => {
    const { daemon, mock, stop, session } = await watched();
    expect(changedInSight(daemon, session(ulid(101)))).toBe(false);
    mock.setStatus(ulid(100), { state: "needs_you", reason: "question" });
    expect(changedInSight(daemon, session(ulid(100)))).toBe(true);
    stop();
    expect(changedInSight(daemon, session(ulid(100)))).toBe(false);
  });
});
