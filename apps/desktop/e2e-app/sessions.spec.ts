import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { askingSession, type Launch, launches } from "./support/agents";
import { keptState, restartApp, sandbox } from "./support/harness";
import { muster, session, sessions, settingsToml, status, until } from "./support/muster";
import { boxOf, dragTo, middleOf } from "./support/pointer";
import { opened } from "./support/record";
import { commit, ghCalls, git, read, repository } from "./support/repos";
import { bareId, goTo, pressOnWindow, theWindow } from "./support/window";

// Sessions and its panels in the real app over a real daemon (PRD §6,
// PRD-15, ARC-24): the tree and its folds, kept across a crash of the app
// and a ⌘Q; the shell on T and a plugin's panel on its key, beside the
// agent and on one line in the tree; the panels dragged, resized and moved
// with m, the layout in settings.toml; full screen; and the stages by hand,
// c, v, o and b, against real git and the stand-in gh.

let repo = "";
let worktree = "";
let agent: Launch;
const TITLE = "Agent in web";

const line = (label: string) => $(`[role="treeitem"][aria-label="${label}"]`);
const lineLike = (start: string) => $(`[role="treeitem"][aria-label^="${start}"]`);
const panel = (name: string) => $(`section.mu-panel[aria-label="${name}"]`);
const statusBar = () => $("footer.mu-statusbar");

/** The lines the app keeps folded, as its file holds them. */
function folds(): string[] {
  const kept = JSON.parse(keptState()["muster.sessions.folds"] ?? "null") as {
    projects: Record<string, string[]>;
  } | null;
  return Object.values(kept?.projects ?? {}).flat();
}

/** A plugin in a folder of the sandbox's own, with a panel on G that says it opened. */
function panelPlugin(): string {
  const folder = join(sandbox().root, "plugins", "probe-panel");
  mkdirSync(folder, { recursive: true });
  writeFileSync(
    join(folder, "muster-plugin.toml"),
    `id = "probe-panel"
name = "Probe panel"
version = "0.1.0"
description = "A panel that says where it opened."
muster = "0.4.0"

[[panels]]
id = "notes"
title = "notes"
on = "session"
command = ["./panel", "{branch}"]
key = "G"
`,
  );
  writeFileSync(
    join(folder, "panel"),
    `#!/bin/sh\necho "notes on $1"\ntouch "$(dirname "$0")/opened"\nexec /bin/sh\n`,
  );
  chmodSync(join(folder, "panel"), 0o755);
  return folder;
}

/**
 * A window wide enough for the agent and a panel beside it, each at the
 * least a panel is given (solve.ts `PANEL_LEAST`): narrower, the pane shows
 * the session alone, as the terminal UI folds a layout its window is too
 * narrow for.
 */
async function roomForPanels(): Promise<void> {
  await browser.setWindowRect(40, 40, 1680, 1000);
}

describe("Sessions and its panels, over the real daemon", () => {
  before(async () => {
    await roomForPanels();
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("web");
    // The project's checks, which leave a file to say they ran.
    commit(repo, "Checks", {
      "muster.toml": '[checks]\ncommands = ["echo checked > checks-ran.txt"]\n',
    });
    // Its origin on GitHub, as far as Muster can tell; pushed to a folder here.
    const bare = join(sandbox().root, "origin.git");
    git(repo, "init", "--quiet", "--bare", bare);
    git(repo, "remote", "add", "origin", "https://github.com/o/app.git");
    git(repo, "config", `url.${bare}.pushInsteadOf`, "https://github.com/o/app.git");
    muster("project", "add", repo);
    muster("worktree", "add", "web", "--branch", "feature/web");
    agent = await askingSession("web", TITLE, "Shall I begin?", { branch: "feature/web" });
    worktree = agent.cwd;
    muster("plugin", "link", panelPlugin(), "--yes");
    await goTo("Sessions");
  });

  it("ARC-24 lists every project, its worktrees and their sessions as the daemon holds them", async () => {
    await expect(line("web")).toHaveAttribute("aria-level", "1");
    await expect(line("main")).toHaveAttribute("aria-level", "2");
    await expect(line("feature/web")).toHaveAttribute("aria-level", "2");
    await expect(lineLike(`${TITLE},`)).toHaveAttribute("aria-level", "3");
    const held = status().projects?.find((project) => project.name === "web");
    expect(held?.workspaces.map((workspace) => workspace.branch).sort()).toEqual([
      "feature/web",
      "main",
    ]);
  });

  it("ARC-24 a fold is kept across a crash of the app and a ⌘Q; the daemon is never told", async () => {
    await line("feature/web").click();
    await pressOnWindow("e");
    await expect(line("feature/web")).toHaveAttribute("aria-expanded", "false");
    await expect(line("feature/web")).toHaveText(expect.stringContaining("1 folded"));
    await expect(lineLike(`${TITLE},`)).not.toBeExisting();
    // Kept by the app as it is folded, in a file of its own (src-tauri/src/kept.rs).
    const workspace = session(TITLE)?.workspace ?? "";
    await until(() => folds().includes(bareId(workspace)), "the fold to be kept");
    const pid = status().daemon?.pid;
    // Ended as a crash ends it, with no word to the window.
    await restartApp();
    await theWindow();
    await roomForPanels();
    await goTo("Sessions");
    await expect(line("feature/web")).toHaveAttribute("aria-expanded", "false");
    await line("feature/web").click();
    await pressOnWindow("→");
    await expect(lineLike(`${TITLE},`)).toBeDisplayed();
    await until(() => !folds().includes(bareId(workspace)), "the unfold to be kept");

    // The project folded, then the app quit as the person quits it.
    await line("web").click();
    await pressOnWindow("e");
    await expect(line("web")).toHaveAttribute("aria-expanded", "false");
    await until(() => folds().length === 1, "the project's fold to be kept");
    await restartApp({}, "quit");
    await theWindow();
    await roomForPanels();
    await goTo("Sessions");
    await expect(line("web")).toHaveAttribute("aria-expanded", "false");
    await expect(line("feature/web")).not.toBeExisting();
    // The daemon ran on, and its session with it.
    expect(status().daemon?.pid).toBe(pid);
    expect(session(TITLE)?.status).toBe("needs_you");
    await line("web").click();
    await pressOnWindow("→");
    await expect(lineLike(`${TITLE},`)).toBeDisplayed();
  });

  it("PRD-15 T opens a real shell beside the agent, on one line under it in the tree", async () => {
    await lineLike(`${TITLE},`).click();
    await pressOnWindow("T");
    const shell = await until(
      () => sessions().find((one) => one.harness === "shell" && one.branch === "feature/web"),
      "the daemon to start a shell in the worktree",
    );
    await expect(panel("Shell")).toBeDisplayed();
    await expect(panel("Shell")).toHaveElementClass("has-keys");
    await $('section.mu-panel[aria-label="Shell"] textarea').addValue("touch shell-was-here");
    await browser.keys("Enter");
    await until(() => existsSync(join(worktree, "shell-was-here")), "the shell to run it");
    await pressOnWindow("Q");
    await expect(lineLike("Shell: ")).toBeDisplayed();
    expect(shell.workspace).toBe(session(TITLE)?.workspace);
  });

  it("PRD-15 a plugin's panel from a local folder opens beside the session on its key", async () => {
    await lineLike(`${TITLE},`).click();
    await pressOnWindow("G");
    await until(
      () => sessions().find((one) => one.harness === "plugin:probe-panel/notes"),
      "the daemon to open the panel",
    );
    await until(
      () => existsSync(join(sandbox().root, "plugins", "probe-panel", "opened")),
      "the panel's program to run",
    );
    await expect(panel("Probe panel")).toBeDisplayed();
    await expect(lineLike("Panel: Probe panel")).toHaveText(expect.stringContaining("G"));
    await pressOnWindow("Q");
  });

  it("ARC-24 panels dragged by their heads and resized by the line between, the layout in settings.toml", async () => {
    await lineLike(`${TITLE},`).click();
    const before = settingsToml();
    // The shell's head dropped on the lower part of the agent's panel: under it.
    const agentPanel = await boxOf('section.mu-panel[aria-label="Claude Code"]');
    await dragTo(
      'section.mu-panel[aria-label="Shell"] .mu-panel-head',
      agentPanel.x + agentPanel.width / 2,
      agentPanel.y + agentPanel.height * 0.85,
    );
    const dropped = await until(() => {
      const now = settingsToml();
      return now !== before && /panels = ".*session \/ shell/.test(now) ? now : null;
    }, "the layout under the agent to be written to settings.toml");
    const shell = await boxOf('section.mu-panel[aria-label="Shell"]');
    const agentNow = await boxOf('section.mu-panel[aria-label="Claude Code"]');
    expect(shell.y).toBeGreaterThan(agentNow.y);
    // The line between them, dragged up, gives the shell more of the place.
    const separator = '[role="separator"][aria-orientation="horizontal"]';
    const bar = await middleOf(separator);
    await dragTo(separator, bar.x, bar.y - 120);
    await until(
      () => settingsToml() !== dropped && /session \d+% \/ shell/.test(settingsToml()),
      "the share the line was dragged to to be written",
    );
    expect(Number(settingsToml().match(/session (\d+)%/)?.[1])).toBeLessThan(50);
    // Esc lets go of a drag, the layout as it was.
    const kept = settingsToml();
    await dragTo(
      'section.mu-panel[aria-label="Shell"] .mu-panel-head',
      shell.x + 5,
      shell.y - 200,
      {
        cancel: true,
      },
    );
    await expect($(".panels-landing")).not.toBeExisting();
    expect(settingsToml()).toBe(kept);
  });

  it("ARC-24 m moves the panels with the keys, and ⏎ keeps the layout", async () => {
    const before = settingsToml();
    await pressOnWindow("m");
    await expect(statusBar()).toHaveText(expect.stringContaining("Moving"));
    await pressOnWindow("→", "Enter");
    await until(() => settingsToml() !== before, "the layout moved to be written");
    expect(settingsToml()).toMatch(/panels = "/);
  });

  it("ARC-24 f fills the window with the session and its panels; Esc gives it back", async () => {
    await lineLike(`${TITLE},`).click();
    await pressOnWindow("f");
    await expect($('nav[aria-label="Screens"]')).not.toBeDisplayed();
    await expect($('[role="tree"]')).not.toBeDisplayed();
    await expect($(".mu-titlebar")).toHaveText(expect.stringContaining(TITLE));
    await expect(panel("Shell")).toBeDisplayed();
    await pressOnWindow("Q", "Esc");
    await expect($('nav[aria-label="Screens"]')).toBeDisplayed();
    await expect($('[role="tree"]')).toBeDisplayed();
  });

  it("PRD §5 c runs the project's checks in the worktree, a session opened with the keys", async () => {
    await line("feature/web").click();
    await pressOnWindow("c");
    await until(
      () => sessions().find((one) => one.harness === "checks" && one.branch === "feature/web"),
      "the daemon to start the checks",
    );
    await until(() => read(worktree, "checks-ran.txt") === "checked\n", "the checks to run");
    await expect(lineLike("Checks,")).toHaveAttribute("aria-selected", "true");
    await expect(statusBar()).toHaveText(expect.stringContaining("give the keys back"));
    await pressOnWindow("Q");
  });

  it("PRD §5 v has Claude Code review the change in plan mode, a session opened with the keys", async () => {
    await line("feature/web").click();
    await pressOnWindow("v");
    const review = await until(
      () => sessions().find((one) => one.harness === "review" && one.branch === "feature/web"),
      "the daemon to start the review",
    );
    const started = await until(() => launches(review.id)[0], "the stand-in Claude Code to start");
    expect(started.agent).toBe("claude");
    expect(started.args).toContain("plan");
    await expect(statusBar()).toHaveText(expect.stringContaining("give the keys back"));
    await pressOnWindow("Q");
  });

  it("PRD §5 o pushes the branch with git and opens a draft with gh; b opens it in the browser", async () => {
    await line("feature/web").click();
    await pressOnWindow("b");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("No pull request from this branch is known; o opens one."),
    );
    commit(worktree, "The web's change", { "web.txt": "a change\n" });
    await pressOnWindow("o");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("Draft pull request #17 is open: https://github.com/o/app/pull/17"),
    );
    expect(git(join(sandbox().root, "origin.git"), "log", "--format=%s", "feature/web")).toContain(
      "The web's change",
    );
    expect(ghCalls().some((call) => call.startsWith("pr create --draft"))).toBe(true);
    await expect(line("feature/web")).toHaveText(expect.stringContaining("#17"));
    await pressOnWindow("b");
    await until(() => opened().includes("https://github.com/o/app/pull/17"), "the address opened");
  });

  it("PRD §5 says why where none can be: on a project's line, and the base branch's own", async () => {
    await line("web").click();
    await pressOnWindow("c");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("Select a worktree, or a session in one, to run the checks there."),
    );
    await line("main").click();
    await pressOnWindow("o");
    await expect(statusBar()).toHaveText(
      expect.stringContaining("main is the branch a pull request would merge into"),
    );
  });
});
