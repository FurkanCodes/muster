import { existsSync } from "node:fs";
import { join } from "node:path";
import { $, expect } from "@wdio/globals";
import { agentOf, ask, change, finish, startAgent, title, work } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, status, until, untilSession } from "./support/muster";
import { changes, commit, ghCalls, repository, tickets } from "./support/repos";
import { goTo, pressOnWindow, row, shownScreen, terminalText, untilFlock } from "./support/window";

// The harness itself (support/): each stand-in does what the tests ask of
// it and the real daemon and window follow, so the specs of every area can
// lean on it.

let repo = "";

describe("The real-app harness", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("app");
    muster("project", "add", repo);
  });

  it("gives the window's WebKit storage a home of the sandbox's own, never the person's", () => {
    // Each spec file's window starts with nothing another left behind.
    expect(existsSync(join(sandbox().env.HOME as string, "Library/WebKit/muster-desktop"))).toBe(
      true,
    );
  });

  it("makes a stand-in Claude Code ask, work and finish, as its hooks tell the daemon", async () => {
    const claude = await startAgent("app", "Claude stand-in");
    expect(claude.agent).toBe("claude");
    expect(claude.id).toHaveLength(36);
    await ask(claude, "May the stand-in run ls?");
    await untilSession("Claude stand-in", "needs_you");
    await untilFlock(0, 1, 0);
    await expect($(".mu-question-ask")).toHaveText("May the stand-in run ls?");
    await work(claude);
    await untilSession("Claude stand-in", "running");
    await untilFlock(1, 0, 0);
    await finish(claude);
    await untilSession("Claude stand-in", "finished");
  });

  it("makes a stand-in Codex ask and work by its window title", async () => {
    const codex = await startAgent("app", "Codex stand-in", { agent: "codex" });
    expect(codex.agent).toBe("codex");
    await ask(codex, "Shall Codex go on?");
    await untilSession("Codex stand-in", "needs_you");
    title(codex, "⠋ Working");
    await untilSession("Codex stand-in", "running");
  });

  it("changes files in the agent's worktree, as git sees them", async () => {
    const claude = await agentOf("Claude stand-in");
    change(claude, "src/new.txt", "written by the stand-in\n");
    expect(changes(claude.cwd)).toContain("?? src/");
    commit(claude.cwd, "The stand-in's commit", { "src/new.txt": "committed\n" });
    expect(changes(claude.cwd)).toEqual([]);
  });

  it("answers for the tracker with the tickets given, and logs each call", () => {
    tickets([{ number: 42, title: "Fix the login redirect", body: "It lands on /404." }]);
    expect(muster("issue", "list", "app")).toContain("Fix the login redirect");
    expect(ghCalls().some((call) => call.startsWith("issue list"))).toBe(true);
  });

  it("presses the window's keys, and reads what a terminal shows", async () => {
    await pressOnWindow("⌘2");
    expect(await shownScreen()).toBe("All sessions");
    await pressOnWindow("⌘1");
    expect(await shownScreen()).toBe("Needs you");
    await goTo("Working");
    await goTo("Needs you");
    // Codex asks again, and is shown with its terminal. Claude's finish is
    // news, and in Needs you too, unless the window had the focus as it
    // finished: what a window looked at shows is never news.
    const codex = await agentOf("Codex stand-in");
    title(codex, "[ ! ] Action Required");
    const id = (await untilSession("Codex stand-in", "needs_you")).id;
    await row("Codex stand-in").click();
    await expect(row("Codex stand-in")).toHaveAttribute("aria-selected", "true");
    await until(
      async () => (await terminalText(id)).includes("Shall Codex go on?"),
      "the terminal to show the question",
    );
  });
});
