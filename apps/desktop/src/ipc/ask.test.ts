import { describe, expect, it } from "vitest";
import { ask, replyAs } from "./ask";
import type { Reply } from "./generated/Reply";

const answering = (reply: Reply) => ({ command: async () => reply });

describe("ARC-24 a command's answer is the reply it asked for, or an error", () => {
  it("gives what the reply carries", async () => {
    const started = { SessionStarted: { id: "ses_1" } } as unknown as Reply;
    await expect(ask(answering(started), "Ping", "SessionStarted")).resolves.toEqual({
      id: "ses_1",
    });
    expect(replyAs(started, "SessionStarted")).toEqual({ id: "ses_1" });
  });

  it("refuses any other answer, saying so", async () => {
    await expect(ask(answering("Done"), "Ping", "SessionStarted", "No session.")).rejects.toThrow(
      "No session.",
    );
    expect(replyAs("Done", "SessionStarted")).toBeUndefined();
  });
});
