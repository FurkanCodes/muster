import { existsSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { askingSession } from "./support/agents";
import { appRuns } from "./support/harness";
import { muster, session, status, until } from "./support/muster";
import { daemonStarts } from "./support/record";
import { repository } from "./support/repos";
import { typeInTerminal, untilFlock } from "./support/window";

// The real app over a real daemon of its own (support/harness.ts makes the
// sandbox): it starts the daemon, connects, shows a session that asks the
// person, takes typing into its terminal, and closing it stops nothing
// (PRD-09). The agent is the stand-in (support/stand-ins.ts), so no model
// is reached.

const TITLE = "Stand-in agent";
let repo = "";

describe("Muster's window over its own daemon", () => {
  before(async () => {
    // Nothing answered as the app opened: it started the daemon itself.
    await until(() => status().running, "the app to start its daemon", 30_000);
    expect(daemonStarts()).toEqual(["daemon --detach"]);
    repo = repository("app");
    muster("project", "add", repo);
    await askingSession("app", TITLE, "Shall the stand-in go on?");
  });

  it("connects and shows the session that asks the person, with its question", async () => {
    await expect($(`h2=${TITLE}`)).toBeDisplayed();
    await untilFlock(0, 1, 0);
    await expect($('section[aria-label="Terminal"]')).toBeDisplayed();
    // Its question, as the link reads it off the stand-in's screen.
    await expect($(".mu-question-ask")).toHaveText("Shall the stand-in go on?");
  });

  it("types into the session's terminal, and what is typed reaches its program", async () => {
    await typeInTerminal("touch typed-in-the-window");
    await until(() => existsSync(join(repo, "typed-in-the-window")), "the shell to run it");
  });

  it("closes, and the daemon and the session run on", async () => {
    const before = status();
    expect(session(TITLE, before)?.status).toBe("needs_you");
    await browser.closeWindow();
    // The window was the app's last: the app ends, and the WebDriver server
    // in it, so there is no session left for the runner to end. (WebdriverIO
    // overwrites protocol commands too; its types list only the others.)
    const overwrite = browser.overwriteCommand.bind(browser) as unknown as (
      name: string,
      command: () => Promise<void>,
    ) => Promise<void>;
    await overwrite("deleteSession", async () => {});
    await until(() => !appRuns(), "the app to end");
    const after = status();
    expect(after.running).toBe(true);
    expect(after.daemon?.pid).toBe(before.daemon?.pid);
    expect(session(TITLE, after)?.status).toBe("needs_you");
  });
});
