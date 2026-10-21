import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";

// The real app over a real daemon of its own (wdio.conf.ts makes the
// sandbox): it starts the daemon, connects, shows a session that asks the
// person, takes typing into its terminal, and closing it stops nothing
// (PRD-09). The stand-in agent is a plain shell (`codex` on the sandbox's
// PATH), so no model is reached.

const repo = process.env.MUSTER_E2E_REPO ?? "";
const TITLE = "Stand-in agent";
const APP = new URL("../src-tauri/target/debug/muster-desktop", import.meta.url).pathname;

/** Whether this worktree's app still runs. */
function appRuns(): boolean {
  try {
    execFileSync("pgrep", ["-f", APP], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** `muster` in the sandbox, as the person's terminal would run it. */
function muster(...args: string[]): string {
  return execFileSync(process.env.MUSTER_BIN ?? "muster", args, { encoding: "utf8" });
}

interface Status {
  running: boolean;
  daemon?: { pid: number };
  projects?: { workspaces: { sessions: { id: string; title: string; status: string }[] }[] }[];
}

function status(): Status {
  try {
    return JSON.parse(muster("status", "--json")) as Status;
  } catch {
    return { running: false };
  }
}

function standIn(state: Status) {
  return state.projects
    ?.flatMap((project) => project.workspaces)
    .flatMap((workspace) => workspace.sessions)
    .find((session) => session.title === TITLE);
}

describe("Muster's window over its own daemon", () => {
  before(async () => {
    // The app has one window. Chosen outright, so the service does not look
    // for the window in focus before each command through Tauri's global
    // API, which the app does not expose.
    const [main] = await browser.getWindowHandles();
    if (main) await browser.switchToWindow(main);
    // Nothing answered as the app opened: it started the daemon itself.
    await browser.waitUntil(() => status().running, {
      timeout: 30_000,
      timeoutMsg: "the app did not start its daemon",
    });
    muster("project", "add", repo);
    muster("session", "start", "app", "--harness", "codex", "--title", TITLE);
  });

  it("connects and shows the session that asks the person, with its question", async () => {
    await expect($(`h2=${TITLE}`)).toBeDisplayed();
    await expect($('[aria-label="0 working, 1 waiting on you, 0 done"]')).toBeExisting();
    await expect($('section[aria-label="Terminal"]')).toBeDisplayed();
    // Its question, as the link reads it off the stand-in's screen.
    await expect($(".mu-question-ask")).toHaveText("Shall the stand-in go on?");
  });

  it("types into the session's terminal, and what is typed reaches its program", async () => {
    await $('section[aria-label="Terminal"]').click();
    // xterm takes text from its textarea, as a paste or an input method
    // gives it; the embedded driver's key events would type each letter
    // twice there (keydown and input), so the line goes in as input and
    // Enter as a key.
    await $('section[aria-label="Terminal"] textarea').addValue("touch typed-in-the-window");
    await browser.keys("Enter");
    await browser.waitUntil(() => existsSync(join(repo, "typed-in-the-window")), {
      timeoutMsg: "the shell did not run what was typed",
    });
  });

  it("closes, and the daemon and the session run on", async () => {
    const before = status();
    expect(standIn(before)?.status).toBe("needs_you");
    await browser.closeWindow();
    // The window was the app's last: the app ends, and the WebDriver server
    // in it, so there is no session left for the runner to end. (WebdriverIO
    // overwrites protocol commands too; its types list only the others.)
    const overwrite = browser.overwriteCommand.bind(browser) as unknown as (
      name: string,
      command: () => Promise<void>,
    ) => Promise<void>;
    await overwrite("deleteSession", async () => {});
    for (let waited = 0; appRuns() && waited < 100; waited += 1) {
      await new Promise((done) => setTimeout(done, 100));
    }
    expect(appRuns()).toBe(false);
    const after = status();
    expect(after.running).toBe(true);
    expect(after.daemon?.pid).toBe(before.daemon?.pid);
    expect(standIn(after)?.status).toBe("needs_you");
  });
});
