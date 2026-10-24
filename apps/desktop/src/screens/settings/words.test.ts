import { describe, expect, it } from "vitest";
import { oneLine } from "../../lib/text";
import {
  changeWords,
  commandLine,
  heard,
  hookWhen,
  hookWords,
  listed,
  problemText,
  problemWord,
  sentence,
  sideBySideLabel,
  soundLabel,
  soundSaid,
  span,
  stepSideBySide,
  stepSound,
  whenWords,
} from "./words";

describe("what Settings says", () => {
  it("takes control characters out of text from outside", () => {
    expect(oneLine("Fix\tthe\nlogin\u001b]0;owned\u0007 now")).toBe("Fix the login]0;owned now");
    expect(sentence("it does not read.")).toBe("it does not read");
  });

  it("lists things as people do", () => {
    expect(listed([])).toBe("");
    expect(listed(["a"])).toBe("a");
    expect(listed(["a", "b"])).toBe("a and b");
    expect(listed(["a", "b", "c"], "or")).toBe("a, b or c");
  });

  it("steps the width a session sits beside the list from, as the terminal UI does", () => {
    expect(stepSideBySide(100, 1)).toBe(110);
    expect(stepSideBySide(100, -1)).toBe(90);
    expect(stepSideBySide(160, 1)).toBe(160);
    expect(stepSideBySide(0, -1)).toBe(0);
    // A width written by hand goes to the choice at or above it.
    expect(stepSideBySide(95, 0)).toBe(100);
    expect(stepSideBySide(500, -1)).toBe(140);
    expect(sideBySideLabel(0)).toBe("Any width");
    expect(sideBySideLabel(120)).toBe("120 columns");
  });

  it("steps a sound round the Mac's default, none and its own (`Sound::step`)", () => {
    expect(stepSound("default", 1)).toBe("none");
    expect(stepSound("none", 1)).toBe("Basso");
    expect(stepSound("default", -1)).toBe("Tink");
    expect(stepSound("Basso", 1)).toBe("Blow");
    expect(stepSound("notification.wav", 1)).toBe("default");
    expect(soundLabel("default")).toBe("The Mac's default");
    expect(soundSaid("none")).toBe("no sound");
    expect(heard("Glass")).toBe(true);
    expect(heard("default")).toBe(false);
    expect(heard("none")).toBe(false);
  });

  it("says a span of time as the core does", () => {
    expect(span(0)).toBe("0 s");
    expect(span(59_999)).toBe("59 s");
    expect(span(60_000)).toBe("1 min");
    expect(span(3_600_000 + 5 * 60_000)).toBe("1 h 05 min");
    expect(span(86_400_000)).toBe("1 day");
    expect(span(3 * 86_400_000 + 1)).toBe("3 days");
  });

  it("writes a command as a shell would read it", () => {
    expect(commandLine(["zoe", "{transcript}", "--follow"])).toBe("zoe {transcript} --follow");
    expect(commandLine(["sh", "-c", "echo 'hi' > x"])).toBe(`sh -c 'echo '\\''hi'\\'' > x'`);
    expect(commandLine(["a\u001bb", ""])).toBe("ab ''");
  });

  it("says what a plugin's hooks hear, short and in a sentence", () => {
    const hook = {
      id: "notify",
      on: ["session.needs_you", "session.finished", "session.error"] as const,
      command: ["./notify"],
      timeout: 15,
      when: { project: [], harness: [], reason: [], away: null },
    };
    expect(hookWords([{ ...hook, on: [...hook.on] }])).toBe("needs you, finished, failed");
    expect(hookWhen([...hook.on])).toBe("when an agent needs you, finishes or fails");
    expect(hookWhen(["session.started", "worktree.created"])).toBe(
      "when an agent starts or a worktree is made",
    );
    expect(
      whenWords({ project: ["muster"], harness: ["claude"], reason: ["permission"], away: true }),
    ).toBe("project muster · harness claude · reason permission · while you are away");
  });

  it("says why a plugin does not run, short and in full", () => {
    expect(problemWord("Changed")).toBe("changed since you approved it");
    expect(problemWord({ Missing: { path: "/x" } })).toBe("its files are gone");
    expect(problemText({ Missing: { path: "/x" } })).toBe("/x is gone");
    expect(problemText("NotApproved")).toMatch(/^it was never approved/);
  });

  it("says a plugin's last change, with when", () => {
    expect(changeWords({ what: "turned_on", by: "command_line", at: 0 }, 120_000)).toBe(
      "turned on 2 min ago from the command line",
    );
    expect(changeWords({ what: "installed", by: "ui", at: 0 }, 1000)).toBe(
      "installed 1 s ago on the Settings screen",
    );
  });
});
