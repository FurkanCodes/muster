import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { ask, change, finish, type Launch, launches, startAgent, work } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, session, status, until, untilSession } from "./support/muster";
import { opened } from "./support/record";
import { git, read, repository } from "./support/repos";
import {
  bareId,
  goTo,
  pressOnWindow,
  type Screen,
  terminalText,
  type,
  typeInTerminal,
  untilFlock,
  untilRowSays,
} from "./support/window";

// Needs you, Not seen and Working in the real app over a real daemon
// (PRD-01, ARC-24): the queue's sections in the model's order, the keys on
// the selected session, its terminal, the banner, This turn from a real
// turn of a stand-in agent, and what the list does to a session, each
// checked in the window and in the daemon, on disk and in git.

let repo = "";
/** The stand-in agents, by what each is for, as each test starts them. */
const agents = {} as Record<
  "asks" | "long" | "lately" | "done" | "later" | "close" | "turn",
  Launch
>;

/** The queue's sessions, in the order shown. */
async function shownNames(screen: Screen = "Needs you"): Promise<string[]> {
  const options = await $$(`[role="listbox"][aria-label="${screen} sessions"] [role="option"]`);
  const names: string[] = [];
  for (const option of options) names.push((await option.getAttribute("aria-label")) ?? "");
  return names;
}

async function untilShown(names: string[], screen: Screen = "Needs you"): Promise<void> {
  await until(
    async () => (await shownNames(screen)).join(" | ") === names.join(" | "),
    `${screen} to list ${names.join(", ")} (it lists ${(await shownNames(screen)).join(", ")})`,
  );
}

const option = (name: string) => $(`[role="option"][aria-label="${name}"]`);

async function untilSelected(name: string): Promise<void> {
  await expect(option(name)).toHaveAttribute("aria-selected", "true");
}

const statusBar = () => $("footer.mu-statusbar");

/**
 * A turn of a stand-in Claude Code titled `title`: once the daemon has
 * kept the files as the turn found them (its timeline's open turn), the
 * prompt taken, `files` written, and its turn ended.
 */
async function turn(title: string, launch: Launch, files: Record<string, string>): Promise<void> {
  const workspace = session(title)?.workspace ?? "";
  await until(
    () => existsSync(join(sandbox().data, "timelines", workspace, "refs", "open")),
    "the daemon to keep the files as the turn found them",
  );
  await work(launch);
  await untilSession(title, "running");
  for (const [file, text] of Object.entries(files)) change(launch, file, text);
  await finish(launch);
}

describe("Needs you, Not seen and Working, over the real daemon", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("app");
    muster("project", "add", repo);
    // Asked first; then one whose turn ends while another is shown; then two at work, the
    // longer first.
    agents.asks = await startAgent("app", "Asks first");
    await ask(agents.asks, "May the stand-in run the tests?");
    await untilSession("Asks first", "needs_you");
    agents.long = await startAgent("app", "Long at work");
    await work(agents.long);
    await untilSession("Long at work", "running");
    agents.lately = await startAgent("app", "Lately at work", { agent: "codex" });
    await work(agents.lately);
    await untilSession("Lately at work", "running");
    agents.done = await startAgent("app", "Done unseen");
    await work(agents.done);
    await finish(agents.done);
    await untilSession("Done unseen", "finished");
  });

  it("PRD-01 lists who asks, then what finished unseen, then the agents at work, the longest first", async () => {
    await untilShown(["Asks first", "Done unseen", "Long at work", "Lately at work"]);
    const heads: string[] = [];
    for (const head of await $$(".queue-list .mu-section")) heads.push(await head.getText());
    expect(heads).toEqual(["Asking you", "Finished, not seen", "Working"]);
    expect(session("Done unseen")?.unseen).toBe(true);
    await untilFlock(2, 1, 1);
    // Working is the agents at work alone, in the daemon's own order.
    await goTo("Working");
    await untilShown(["Long at work", "Lately at work"], "Working");
    await goTo("Needs you");
  });

  it("PRD-01 the asking session's pane says what it waits for, over its live terminal", async () => {
    await untilSelected("Asks first");
    await expect($("h2=Asks first")).toBeDisplayed();
    await expect($(".session-banner")).toHaveText(expect.stringContaining("Needs you:"));
    await expect($(".mu-question-ask")).toHaveText("May the stand-in run the tests?");
    const id = session("Asks first")?.id ?? "";
    await until(
      async () => (await terminalText(id)).includes("May the stand-in run the tests?"),
      "the terminal to show the question",
    );
  });

  it("ARC-24 j and k move the selection; showing what finished unseen clears its news", async () => {
    await pressOnWindow("j");
    await untilSelected("Done unseen");
    await until(() => session("Done unseen")?.unseen === false, "the news to be seen");
    // It keeps its place until another is chosen.
    await untilShown(["Asks first", "Done unseen", "Long at work", "Lately at work"]);
    await pressOnWindow("k");
    await untilSelected("Asks first");
    await untilShown(["Asks first", "Long at work", "Lately at work"]);
  });

  it("PRD-01 Not seen lists what finished while the person looked elsewhere, until it is shown", async () => {
    agents.later = await startAgent("app", "Finished later");
    await work(agents.later);
    await finish(agents.later);
    await untilSession("Finished later", "finished");
    await until(() => session("Finished later")?.unseen, "the daemon to keep it as news");
    await goTo("Not seen");
    await untilShown(["Finished later"], "Not seen");
    await untilSelected("Finished later");
    await until(() => session("Finished later")?.unseen === false, "its news to be seen");
    await goTo("Needs you");
    await untilSelected("Asks first");
  });

  it("ARC-24 ⏎ gives the terminal the keys, typing reaches the agent, and Q gives them back", async () => {
    await pressOnWindow("Enter");
    await expect(statusBar()).toHaveText(expect.stringContaining("give the keys back"));
    await typeInTerminal("touch typed-from-the-queue");
    await until(
      () => existsSync(join(repo, "typed-from-the-queue")),
      "the agent's shell to run it",
    );
    await pressOnWindow("Q");
    await expect(statusBar()).toHaveText(expect.stringContaining("rename"));
  });

  it("ARC-24 l goes to the session that came to need the person last", async () => {
    await ask(agents.long, "Shall the long one go on?");
    await untilSession("Long at work", "needs_you");
    // The window sees it ask before it is answered: drawn among who asks (an
    // item, not a row). At work, it was already second in the list, so its
    // place says nothing; and a window sent the ask and the answer in one
    // patch never saw it need the person.
    await expect(option("Long at work")).toHaveElementClass("mu-item");
    await work(agents.long);
    await untilSession("Long at work", "running");
    // At work again since just now: the newest at work, so after the other.
    await untilShown(["Asks first", "Lately at work", "Long at work"]);
    await pressOnWindow("l");
    await untilSelected("Long at work");
    await pressOnWindow("k", "k");
    await untilSelected("Asks first");
  });

  it("PRD-09 R renames the chat in place, and the daemon keeps the name", async () => {
    await pressOnWindow("R");
    await type('input[name="session-name"]', "Asks, renamed");
    await browser.keys("Enter");
    await until(() => session("Asks, renamed"), "the daemon to keep the new name");
    await expect($("h2=Asks, renamed")).toBeDisplayed();
    expect(muster("session", "list")).toContain("Asks, renamed");
  });

  it("PRD-09 s stops the session after asking, and r starts it again", async () => {
    await pressOnWindow("s");
    await expect($("dialog")).toHaveText(expect.stringContaining("Stop Asks, renamed?"));
    await $("button=Stop session").click();
    await untilSession("Asks, renamed", "exited");
    const id = session("Asks, renamed")?.id ?? "";
    expect(launches(id)).toHaveLength(1);
    // Ended, it is no longer in Needs you: All sessions holds it.
    await goTo("All sessions");
    const line = $(`[role="treeitem"][id="tree-${bareId(id)}"]`);
    // r on a session the window still shows at work says so and starts nothing.
    await untilRowSays("Asks, renamed", /^(ended|exited)/);
    await line.click();
    await expect(line).toHaveAttribute("aria-selected", "true");
    await pressOnWindow("r");
    await until(() => launches(id).length === 2, "the agent to start again");
    await until(() => session("Asks, renamed")?.status !== "exited", "the daemon to run it again");
    await goTo("Needs you");
  });

  it("PRD-09 x closes a session: asked first, stopped, then off the daemon's list", async () => {
    agents.close = await startAgent("app", "To be closed");
    await ask(agents.close, "Close me?");
    await untilSession("To be closed", "needs_you");
    await option("To be closed").click();
    await untilSelected("To be closed");
    await pressOnWindow("x");
    await expect($("dialog")).toHaveText(expect.stringContaining("Close To be closed?"));
    await $("button=Close session").click();
    await until(() => !session("To be closed"), "the daemon to take it off its list");
    expect(muster("session", "list")).not.toContain("To be closed");
    await until(async () => !(await option("To be closed").isExisting()), "the row to go");
  });

  it("PRD-14 This turn shows a real turn's files; See the diff and Put back open the changes", async () => {
    muster("worktree", "add", "app", "--branch", "feature/turn");
    agents.turn = await startAgent("app", "Takes a turn", { branch: "feature/turn" });
    await ask(agents.turn, "Begin?");
    await untilSession("Takes a turn", "needs_you");
    await option("Takes a turn").click();
    await untilSelected("Takes a turn");
    await turn("Takes a turn", agents.turn, {
      "notes.txt": "the turn's words\n",
      "README.md": "changed\n",
    });
    await untilSession("Takes a turn", "finished");
    const thisTurn = $('section[aria-label="This turn"]');
    await expect(thisTurn.$('button[aria-label="notes.txt, its diff"]')).toBeDisplayed();
    await expect(thisTurn.$('button[aria-label="README.md, its diff"]')).toBeDisplayed();

    await thisTurn.$("button=See the diff").click();
    const changes = $('section[aria-label="Changes"]');
    await expect(changes).toBeDisplayed();
    await expect($('section[aria-label="Diff"]')).toHaveText(
      expect.stringContaining("the turn's words"),
    );
    await pressOnWindow("d");
    await expect(changes).not.toBeDisplayed();

    await thisTurn.$("button=Put back").click();
    await expect($("dialog")).toHaveText(expect.stringContaining("Put the files back as before"));
    await $("button=Put back the files").click();
    await expect(statusBar()).toHaveText(
      expect.stringContaining("The files are as they were before #1."),
    );
    await expect($("dialog")).not.toBeExisting();
    const cwd = agents.turn.cwd;
    await until(() => read(cwd, "notes.txt") === null, "notes.txt to be gone again");
    expect(read(cwd, "README.md")).toBe("test\n");
  });

  it("PRD-07 b opens the session's pull request in the browser; with none, says o opens one", async () => {
    await pressOnWindow("d");
    await pressOnWindow("b");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("No pull request from this branch is known; o opens one."),
    );
    // The branch pushed and its draft opened from the terminal, as the person might.
    const bare = join(repo, "..", "origin.git");
    git(repo, "init", "--quiet", "--bare", bare);
    git(repo, "remote", "add", "origin", "https://github.com/o/app.git");
    git(repo, "config", `url.${bare}.pushInsteadOf`, "https://github.com/o/app.git");
    writeFileSync(join(agents.turn.cwd, "committed.txt"), "on the branch\n");
    git(agents.turn.cwd, "add", "committed.txt");
    git(agents.turn.cwd, "commit", "--quiet", "--message", "On the branch");
    muster("pr", "feature/turn");
    await expect($(".mu-pane-meta")).toHaveText(expect.stringContaining("#17"));
    await pressOnWindow("b");
    await until(() => opened().includes("https://github.com/o/app/pull/17"), "the address opened");
  });
});
