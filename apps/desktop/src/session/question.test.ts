import { describe, expect, it } from "vitest";
import { plainText, questionOf } from "./question";

const asking = (text: string) => ({
  status: { state: "needs_you" as const, reason: "permission" as const },
  activity: { text, since: 0 },
});

describe("ARC-24 a session's question, as its item and banner show it", () => {
  it("is what the agent reports it waits for, until the agent's own lines come", () => {
    expect(questionOf(asking("wants to run cargo build"))).toEqual(["wants to run cargo build"]);
    expect(questionOf({ status: asking("").status, activity: null })).toEqual([
      "wants your permission",
    ]);
  });

  it("PRD-06 shows no escape or control character a program wrote", () => {
    expect(
      plainText("\u001b]8;;https://x\u0007link\u001b]8;;\u0007 \u001b[31mred\u001b[0m\u0007"),
    ).toBe("link red");
  });

  it("is the agent's own last lines off its screen once they come", () => {
    const screen = ["  Allow Codex to run this command?", "  $ cargo build", "  › Yes", "    No"];
    expect(questionOf(asking("wants to run cargo build"), screen)).toEqual([
      "Allow Codex to run this command?",
      "$ cargo build",
      "› Yes",
      "  No",
    ]);
    expect(questionOf(asking("wants to run cargo build"), ["", "  "])).toEqual([
      "wants to run cargo build",
    ]);
    expect(questionOf(asking(""), ["\u001b[33mWhich?\u001b[0m"])).toEqual(["Which?"]);
  });

  it("keeps its last four lines and drops empty ones", () => {
    expect(questionOf(asking("one\n\ntwo\nthree\nfour\nfive"))).toEqual([
      "two",
      "three",
      "four",
      "five",
    ]);
  });
});
