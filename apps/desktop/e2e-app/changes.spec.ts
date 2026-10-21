import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { $, $$, browser, expect } from "@wdio/globals";
import { ask, change, finish, type Launch, startAgent, work } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, session, status, until, untilSession } from "./support/muster";
import { commit, git, log, read, remove, repository, write } from "./support/repos";
import { goTo, pressOnWindow } from "./support/window";

// The changes view in the real app over real git (PRD-14, ARC-24, ARC-25):
// Not committed and On the branch list the worktree's files as git has
// them, each file's diff is git's, Turns are a stand-in agent's real turns,
// putting one back changes the files (and with b the branch) on disk, the
// daemon's refusals are said, r asks git again, and two worktrees changing
// the same file show the overlap, then the conflict.

let repo = "";
let one: Launch;
let two: Launch;

/** Twenty numbered lines, the `changed` ones said otherwise. */
function lines(changed: Record<number, string> = {}): string {
  return `${Array.from({ length: 20 }, (_, n) => changed[n + 1] ?? `line ${n + 1}`).join("\n")}\n`;
}

const statusBar = () => $("footer.mu-statusbar");
const option = (name: string) => $(`[role="option"][aria-label="${name}"]`);
const view = (name: string) => $('section[aria-label="Changes"]').$(`button*=${name}`);

/** The rows of the listbox `label`, by their names: a file's its label, a turn's its words. */
async function rowsOf(label: string): Promise<string[]> {
  const names: string[] = [];
  for (const row of await $$(`[role="listbox"][aria-label="${label}"] [role="option"]`))
    names.push((await row.getAttribute("aria-label")) ?? (await row.getText()));
  return names;
}

/** Where the turn of `title` under way began, as its timeline keeps it (`refs/open/<session>`); null with none under way. */
function openTurn(title: string): string | null {
  const found = session(title);
  const ref = join(
    sandbox().data,
    "timelines",
    found?.workspace ?? "",
    "refs",
    "open",
    found?.id ?? "",
  );
  return found && existsSync(ref) ? readFileSync(ref, "utf8") : null;
}

/** The agent asks `question`, which begins its turn: once the daemon has kept the files as the turn found them. */
async function begin(title: string, launch: Launch, question: string): Promise<void> {
  const before = openTurn(title);
  await ask(launch, question);
  await untilSession(title, "needs_you");
  await until(() => {
    const now = openTurn(title);
    return now !== null && now !== before;
  }, "the daemon to keep the files as the turn found them");
}

/** The agent's turn, begun: the prompt taken, `files` written, and committed as `message` if given; the turn ended. */
async function turn(
  title: string,
  launch: Launch,
  files: Record<string, string>,
  message?: string,
): Promise<void> {
  await work(launch);
  await untilSession(title, "running");
  for (const [file, text] of Object.entries(files)) change(launch, file, text);
  if (message) {
    git(launch.cwd, "add", "--", ...Object.keys(files));
    git(launch.cwd, "commit", "--quiet", "--message", message);
  }
  await finish(launch);
  await untilSession(title, "finished");
}

/** The rows of Claude Code's turns under Turns, the newest first. */
const claudeTurns = () =>
  $$(
    '//*[@role="listbox"][@aria-label="Turns"]//*[@role="option"][contains(., "Claude Code, turn")]',
  );

/** Selects the newest of Claude Code's turns. */
async function newestTurn(): Promise<void> {
  const [newest] = await claudeTurns();
  if (!newest) throw new Error("no turn of Claude Code is listed");
  await newest.click();
}

/** Shows the changes of the session `title`, from the queue. */
async function changesOf(title: string): Promise<void> {
  await goTo("All sessions");
  await $(`[role="treeitem"][aria-label^="${title},"]`).click();
  const shown = await $('section[aria-label="Changes"]').isExisting();
  if (!shown) await pressOnWindow("d");
  await expect($('section[aria-label="Changes"]')).toBeDisplayed();
}

describe("The changes view, over real git", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("lib");
    commit(repo, "The files", {
      "src/a.txt": lines(),
      "old.txt": "to be renamed\n",
      "gone.txt": "to be deleted\n",
      "shared.txt": lines(),
    });
    muster("project", "add", repo);
    muster("worktree", "add", "lib", "--branch", "feature/one");
    muster("worktree", "add", "lib", "--branch", "feature/two");
    one = await startAgent("lib", "One", { branch: "feature/one" });
    two = await startAgent("lib", "Two", { branch: "feature/two" });
    // Each asks, and its first turn ends with nothing changed.
    for (const [title, launch] of [
      ["One", one],
      ["Two", two],
    ] as const) {
      await begin(title, launch, "Begin?");
      await finish(launch);
      await untilSession(title, "finished");
    }
  });

  it("PRD-14 Not committed lists the worktree's files as git has them, with their lines", async () => {
    const cwd = one.cwd;
    write(cwd, "src/a.txt", lines({ 3: "line three", 21: "" }).replace(/\n$/, "\nline 21\n"));
    write(cwd, "new.txt", "one\ntwo\nthree\n");
    remove(cwd, "gone.txt");
    git(cwd, "mv", "old.txt", "renamed.txt");
    write(cwd, "logo.bin", "\u0000\u0001\u0002binary\u0000");
    write(cwd, "big.txt", `${"a long line of text, again and again\n".repeat(20_000)}`);
    await changesOf("One");
    await until(async () => (await rowsOf("Files not committed")).length === 6, "six files");
    expect((await rowsOf("Files not committed")).sort()).toEqual(
      [
        "modified: src/a.txt",
        "untracked: new.txt",
        "deleted: gone.txt",
        "renamed: old.txt → renamed.txt",
        "untracked: logo.bin",
        "untracked: big.txt",
      ].sort(),
    );
    await expect(option("modified: src/a.txt")).toHaveText(expect.stringContaining("+2−1"));
    await expect(option("untracked: new.txt")).toHaveText(expect.stringContaining("+3"));
    await expect(view("Not committed")).toHaveText(expect.stringContaining("6"));
  });

  it("PRD-14 a file's diff is git's: lines added and removed, a rename, a binary, one too long", async () => {
    await option("modified: src/a.txt").click();
    const diff = $('section[aria-label="Diff"]');
    await expect(diff.$(".mu-dl--add*=line three")).toBeExisting();
    await expect(diff.$(".mu-dl--del*=line 3")).toBeExisting();
    await expect(diff.$(".mu-dl--add*=line 21")).toBeExisting();
    await option("renamed: old.txt → renamed.txt").click();
    // Moved as it was: git shows no lines, and the row says from where.
    await expect($(".mu-diffview-head")).toHaveText(expect.stringContaining("renamed.txt"));
    await expect(option("renamed: old.txt → renamed.txt")).toHaveText(
      expect.stringContaining("old.txt → renamed.txt"),
    );
    await option("untracked: logo.bin").click();
    await expect($("p=binary: git shows no lines for it.")).toBeDisplayed();
    await option("untracked: big.txt").click();
    await expect($("p=… cut here: the rest is longer than Muster shows.")).toBeDisplayed();
    await option("deleted: gone.txt").click();
    await expect(diff.$(".mu-dl--del*=to be deleted")).toBeExisting();
  });

  it("ARC-24 r asks git again, and what changed outside the app is listed", async () => {
    write(one.cwd, "later.txt", "written outside\n");
    await pressOnWindow("r");
    await until(
      async () => (await rowsOf("Files not committed")).includes("untracked: later.txt"),
      "later.txt to be listed",
    );
    // The work not committed goes, to keep the turns that follow plain.
    git(one.cwd, "reset", "--quiet", "--hard");
    git(one.cwd, "clean", "--quiet", "-fdx");
    await pressOnWindow("r");
    await expect($("p*=Nothing here is waiting to be committed.")).toBeDisplayed();
  });

  it("PRD-14 On the branch lists its commits since the base and their files", async () => {
    commit(one.cwd, "A commit on one", { "on-branch.txt": "committed\n" });
    await pressOnWindow("⇥", "r");
    await expect(view("On the branch")).toHaveAttribute("aria-pressed", "true");
    await expect($(".changes-commits")).toHaveText(expect.stringContaining("A commit on one"));
    await until(
      async () => (await rowsOf("Files on the branch")).includes("added: on-branch.txt"),
      "the branch's file to be listed",
    );
    await option("added: on-branch.txt").click();
    await expect($('section[aria-label="Diff"]')).toHaveText(expect.stringContaining("committed"));
  });

  it("PRD-14 Turns are the agent's real turns, each with its diff", async () => {
    await begin("One", one, "Shall I write?");
    await turn("One", one, { "turn-one.txt": "the first turn\n" });
    await begin("One", one, "Again?");
    await turn("One", one, { "turn-two.txt": "the second turn\n" }, "The second turn's commit");
    await pressOnWindow("⇥");
    await expect(view("Turns")).toHaveAttribute("aria-pressed", "true");
    await until(async () => (await claudeTurns().length) >= 2, "the two turns");
    const [second, first] = await claudeTurns();
    await expect(second).toHaveText(expect.stringContaining("1 file +1"));
    await first?.click();
    await expect($('section[aria-label="Diff"]')).toHaveText(
      expect.stringContaining("the first turn"),
    );
    await newestTurn();
    await expect($('section[aria-label="Diff"]')).toHaveText(
      expect.stringContaining("the second turn"),
    );
  });

  it("PRD-14 refused while an agent works there, said as the daemon says it", async () => {
    await begin("One", one, "Shall I go on?");
    await newestTurn();
    await pressOnWindow("z");
    await expect($("dialog")).toBeDisplayed();
    await browser.keys("y");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("One is at work there: put the files back once it has stopped"),
    );
    await finish(one);
    await untilSession("One", "finished");
  });

  it("PRD-14 ⏎ puts the files back only: the turn's file goes, the branch stays", async () => {
    const head = git(one.cwd, "rev-parse", "HEAD");
    await newestTurn();
    await pressOnWindow("z");
    await expect($("dialog")).toHaveText(expect.stringContaining("Put the files back as before"));
    await browser.keys("Enter");
    await expect(statusBar()).toHaveText(expect.stringContaining("The files are as they were"));
    expect(read(one.cwd, "turn-two.txt")).toBeNull();
    expect(read(one.cwd, "turn-one.txt")).toBe("the first turn\n");
    expect(git(one.cwd, "rev-parse", "HEAD")).toBe(head);
  });

  it("PRD-14 the same again is refused: the files are already as they were", async () => {
    await newestTurn();
    await pressOnWindow("z");
    await expect($("dialog")).toBeDisplayed();
    await browser.keys("y");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("the files are already as they were before entry"),
    );
  });

  it("PRD-14 b puts the files and the branch back: the turn's commit leaves the branch", async () => {
    // What the person put back is brought back first, so the turn's commit is the branch's again.
    expect(log(one.cwd)[0]).toBe("The second turn's commit");
    await newestTurn();
    await pressOnWindow("z");
    await expect($("dialog")).toHaveText(expect.stringContaining("Put back with the branch"));
    await browser.keys("b");
    await expect(statusBar()).toHaveText(expect.stringContaining("and the branch with them"));
    expect(log(one.cwd)[0]).toBe("A commit on one");
    expect(read(one.cwd, "turn-two.txt")).toBeNull();
  });

  it("ARC-24 two worktrees changing the same file: the overlap, then the conflict", async () => {
    await begin("One", one, "Share?");
    await turn("One", one, { "shared.txt": lines({ 1: "one's first line" }) });
    await begin("Two", two, "Share too?");
    await turn("Two", two, { "shared.txt": lines({ 20: "two's last line" }) });
    await changesOf("Two");
    const banner = $(".mu-banner");
    await expect(banner).toHaveText(
      expect.stringContaining("Overlap with ⎇ feature/one: both change shared.txt"),
    );
    await begin("Two", two, "Again?");
    await turn("Two", two, {
      "shared.txt": lines({ 1: "two's first line", 20: "two's last line" }),
    });
    await expect(banner).toHaveText(
      expect.stringContaining("Conflict with ⎇ feature/one: would conflict in shared.txt"),
    );
  });
});
