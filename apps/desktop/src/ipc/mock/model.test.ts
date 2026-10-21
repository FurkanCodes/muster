import { describe, expect, it } from "vitest";
import type { StateMessage } from "../generated/desktop/StateMessage";
import { MockBridge } from "./bridge";
import { derive, lastLines } from "./model";
import { ulid, workingDay } from "./scenario";

describe("a question, as the Rust side reads it off a session's screen", () => {
  it("is the last four lines with anything on them", () => {
    expect(
      lastLines(
        "Building\r\n\r\n  Allow Codex to run this command?\r\n\r\n  $ cargo build\r\n  › Yes\r\n    No\r\n",
      ),
    ).toEqual(["  Allow Codex to run this command?", "  $ cargo build", "  › Yes", "    No"]);
  });

  it("drops colours and control characters, and reads a line rewritten in place", () => {
    expect(lastLines("\x1b[33mWhich?\x1b[0m\x07\r\nfirst draft\rsecond\r\n")).toEqual([
      "Which?",
      "seconddraft",
    ]);
    expect(lastLines("\x1b]0;[ ! ] Action Required\x07Proceed?")).toEqual(["Proceed?"]);
  });

  it("is asked only of sessions waiting on the person, and only while the window is seen", () => {
    const world = workingDay();
    const asking = derive(world, new Map()).questions;
    expect(Object.keys(asking).sort()).toEqual([ulid(101), ulid(200)]);
    expect(asking[ulid(200)]).toEqual([
      "✦ Two worktrees build lamphaus at once, each with its own cache.",
      "  Where should the shared cache live?",
      "  › ~/.gradle/caches",
      "    ~/code/lamphaus/.gradle-shared",
    ]);
    expect(derive(world, new Map(), false).questions).toEqual({});
  });
});

describe("the mock daemon's questions", () => {
  function opened() {
    const bridge = new MockBridge(workingDay());
    const told: StateMessage[] = [];
    void bridge.open((message) => told.push(message));
    return { bridge, told };
  }

  const lastQuestions = (told: StateMessage[]) => {
    const patches = told.flatMap((message) => (message.kind === "patch" ? [message.patch] : []));
    return patches.at(-1)?.derived.questions;
  };

  it("follow what an asking session writes", async () => {
    const { bridge, told } = opened();
    await Promise.resolve();
    bridge.output(ulid(200), "\r\n    somewhere else");
    expect(lastQuestions(told)?.[ulid(200)]?.at(-1)).toBe("    somewhere else");
  });

  it("are held while the window cannot be seen and told as it is seen again", async () => {
    const { bridge, told } = opened();
    await Promise.resolve();
    await bridge.looking(false, null, false);
    told.length = 0;
    bridge.output(ulid(200), "\r\n    unseen");
    expect(told).toEqual([]);
    await bridge.looking(true, null, true);
    expect(lastQuestions(told)?.[ulid(200)]?.at(-1)).toBe("    unseen");
  });
});
