import { existsSync, mkdirSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { $, browser, expect } from "@wdio/globals";
import { agentOf, finish, launches } from "./support/agents";
import { sandbox } from "./support/harness";
import { muster, project, session, status, until, untilSession } from "./support/muster";
import { branches, commit, git, log, repository, write } from "./support/repos";
import {
  alertIn,
  button,
  dialog,
  field,
  fill,
  noDialog,
  option,
  press,
  pressOnWindow,
  radio,
  selectRow,
  shownScreen,
  untilRowSays,
} from "./support/window";

// The flows over the real daemon (PRODUCT.md §6a, design/screens/Palette,
// NewSession, AddProject, Dialogs, Welcome): the command palette and every
// key, new sessions of each agent and a shell, new worktrees, adding a
// project, the dialogs that stop, restart, tidy, move and remove, and the
// first run. Each test does what the person does and checks what the daemon,
// git and the stand-ins then hold.

const palette = () => dialog("Command palette");
const help = () => dialog("Every key");
const paletteField = () => field(palette(), "Go to or do");

/** Opens the palette and types `text` in it. */
async function find(text: string): Promise<void> {
  await pressOnWindow("⌘K");
  await expect(palette()).toBeDisplayed();
  await fill(paletteField(), text);
}

/** Opens the new-session dialog on `n` with the row named `on` selected. */
async function newSession(on: string) {
  await selectRow(on);
  await press("n");
  const shown = dialog("New session");
  await expect(shown).toBeDisplayed();
  return shown;
}

let app = "";

describe("The flows", () => {
  before(async () => {
    await until(() => status().running, "the app to start its daemon", 30_000);
  });

  describe("the first run", () => {
    it("Kep welcomes with no projects, and a adds one through the folder browser", async () => {
      app = repository("app");
      const home = sandbox().env.HOME as string;
      for (const name of ["lib", "notes"]) {
        mkdirSync(join(home, "code", name), { recursive: true });
      }
      git(join(home, "code", "lib"), "init", "--quiet", "--initial-branch", "main");
      git(join(home, "code", "lib"), "commit", "--quiet", "--allow-empty", "--message", "first");
      await expect($("h1=Add a project to start")).toBeDisplayed();
      await expect($('img[alt="Kep, Muster\'s sheepdog"]')).toBeDisplayed();
      await expect($("main")).toHaveText(
        expect.stringContaining("Or drop a folder on this window."),
      );

      await pressOnWindow("a");
      const adding = dialog("Add a project");
      await expect(adding).toBeDisplayed();
      const folder = field(adding, "Folder");
      // ~ is the person's home: its folders, from the app's list_folders.
      await fill(folder, "~/code/");
      await expect(option(adding, "lib")).toBeDisplayed();
      await expect(option(adding, "notes")).toBeDisplayed();
      await fill(folder, "~/code/li");
      await expect(button(adding, "Add lib")).toBeDisplayed();
      await press("Enter");
      await until(noDialog, "the dialog to close");
      const added = await until(() => project("lib"), "the project lib");
      expect(added.root).toBe(realpathSync(join(home, "code", "lib")));
      await expect($('nav[aria-label="Screens"]').$("button*=lib")).toBeDisplayed();

      // The same again is refused, in the dialog.
      await pressOnWindow("a");
      await fill(field(dialog("Add a project"), "Folder"), "~/code/lib");
      await press("Enter");
      await expect(dialog("Add a project")).toHaveText(
        expect.stringContaining("is already a project"),
      );
      await press("Escape");
      await until(noDialog, "the dialog to close");
    });

    it("adds a folder dropped on the window", async () => {
      await browser.execute(
        (path) =>
          (
            window as unknown as {
              __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
            }
          ).__TAURI_INTERNALS__.invoke("plugin:event|emit", {
            event: "tauri://drag-drop",
            payload: { paths: [path], position: { x: 400, y: 300 } },
          }),
        app,
      );
      await until(() => project("app"), "the dropped folder to be added");
      await expect($('nav[aria-label="Screens"]').$("button*=app")).toBeDisplayed();
    });
  });

  describe("the command palette", () => {
    it("opens on ⌘K with its groups, goes to a session, and closes on ⎋", async () => {
      muster("session", "start", "app", "--harness", "claude", "--title", "Palette agent");
      await agentOf("Palette agent");
      await pressOnWindow("⌘K");
      await expect(palette()).toBeDisplayed();
      const groups = await palette()
        .$$(".mu-palette-group")
        .map((group) => group.getText());
      expect(groups).toEqual(expect.arrayContaining(["Anywhere", "Sessions", "Settings"]));
      await press("Escape");
      await until(noDialog, "the palette to close");

      await find("palette ag");
      await expect(option(palette(), "Palette agent")).toBeDisplayed();
      await press("Enter");
      await until(noDialog, "the palette to close");
      expect(await shownScreen()).toBe("Sessions");
      await expect(
        $('//*[@role="treeitem"][starts-with(@aria-label, "Palette agent,")]'),
      ).toHaveAttribute("aria-selected", "true");
    });

    it("runs an action on what is selected as its key would", async () => {
      await selectRow("Palette agent");
      await find("stop");
      await expect(option(palette(), "Stop the session")).toBeDisplayed();
      await press("Enter");
      const stop = dialog("Stop Palette agent?");
      await expect(stop).toBeDisplayed();
      await press("Enter");
      await untilSession("Palette agent", "exited");
    });

    it("goes to a setting's page and to a project's page", async () => {
      await find("naming");
      await option(palette(), "Naming chats").click();
      await expect($(".mu-settings-title")).toHaveText("Naming chats");
      await find("app");
      await option(palette(), "app").click();
      await expect($(".mu-settings-title")).toHaveText("app");
    });

    it("says what it found nothing for", async () => {
      await find("nothing holds this");
      await expect(palette()).toHaveText(
        expect.stringContaining("No action, session, ticket or setting holds"),
      );
      await press("Escape");
    });
  });

  describe("every key", () => {
    it("h lists the keys of the screen in view, narrows by words, and ⏎ closes it", async () => {
      await pressOnWindow("⌘2");
      await pressOnWindow("h");
      await expect(help()).toBeDisplayed();
      const headings = await help()
        .$$("h3")
        .map((heading) => heading.getText());
      expect(headings).toEqual(["On the queue", "In a session", "Anywhere"]);
      await fill(field(help(), "Find a key"), "rename");
      await expect(help().$$("li")).toBeElementsArrayOfSize(1);
      await press("Enter");
      await until(noDialog, "the keys to close");
      await pressOnWindow("⌘3");
      await pressOnWindow("?");
      await expect(help()).toBeDisplayed();
      const tickets = await help()
        .$$("h3")
        .map((heading) => heading.getText());
      expect(tickets[0]).toBe("On the tickets");
      await press("Escape");
      await until(noDialog, "the keys to close");
    });
  });

  describe("a new session", () => {
    it("starts Claude Code in a new worktree with a first prompt, which names it", async () => {
      const shown = await newSession("app");
      await fill(field(shown, "First prompt"), "Fix the login redirect\nIt lands on /404.");
      await button(shown, "A new worktree").click();
      await fill(field(shown, "Branch"), "fix/login");
      await radio(shown, "Claude Code").click();
      await button(shown, "Start Claude Code").click();
      await until(noDialog, "the dialog to close");
      const started = await until(() => session("Fix the login redirect"), "the session");
      expect(started.harness).toBe("claude");
      expect(started.branch).toBe("fix/login");
      expect(branches(app)).toContain("fix/login");
      const launch = await agentOf("Fix the login redirect");
      expect(launch.args.join(" ")).toContain("Fix the login redirect");
      // Selected once the daemon has told it, its terminal shown with the keys.
      await expect(
        $('//*[@role="treeitem"][starts-with(@aria-label, "Fix the login redirect,")]'),
      ).toHaveAttribute("aria-selected", "true");
      await expect($("footer")).toHaveText(expect.stringContaining("give the keys back"));
      await press("Q");
    });

    for (const [agent, name] of [
      ["codex", "Codex"],
      ["gemini", "Gemini CLI"],
      ["opencode", "OpenCode"],
      ["copilot", "Copilot CLI"],
    ] as const) {
      it(`starts ${name} in the project's checkout, with no first prompt`, async () => {
        const before = new Set(sessionIds());
        const shown = await newSession("app");
        await button(shown, "Its checkout").click();
        await radio(shown, name).click();
        await button(shown, `Start ${name}`).click();
        await until(noDialog, "the dialog to close");
        const made = await until(
          () => sessionsNow().find((one) => !before.has(one.id)),
          `the ${name} session`,
        );
        expect(made.harness).toBe(agent);
        expect(made.branch).toBe("main");
        const started = await until(() => launches(made.id)[0], `${name} to start`);
        expect(started.agent).toBe(agent);
      });
    }

    it("starts a shell, which takes no first prompt", async () => {
      const before = new Set(sessionIds());
      const shown = await newSession("app");
      await button(shown, "Its checkout").click();
      await radio(shown, "A shell").click();
      await expect(field(shown, "First prompt")).toBeDisabled();
      await button(shown, "Start a shell").click();
      await until(noDialog, "the dialog to close");
      const made = await until(() => sessionsNow().find((one) => !before.has(one.id)), "the shell");
      expect(made.harness).toBe("shell");
    });

    it("keeps the daemon's refusal in the dialog, and starts nothing", async () => {
      const before = sessionIds().length;
      const shown = await newSession("app");
      await button(shown, "A new worktree").click();
      await fill(field(shown, "Branch"), "not a branch");
      await radio(shown, "A shell").click();
      await button(shown, "Start a shell").click();
      await expect(alertIn(shown)).toHaveText('"not a branch" is not a valid branch name');
      expect(sessionIds()).toHaveLength(before);
      await press("Escape");
      await until(noDialog, "the dialog to close");
    });

    it("w makes a worktree on a branch", async () => {
      await selectRow("app");
      await press("w");
      const shown = dialog("New worktree");
      await expect(shown).toBeDisplayed();
      await fill(field(shown, "Branch"), "spike/webgl");
      await button(shown, "Make the worktree").click();
      await until(noDialog, "the dialog to close");
      await until(
        () =>
          project("app")?.workspaces.find(
            (one) => one.branch === "spike/webgl" && one.kind === "worktree",
          ),
        "the worktree",
      );
      expect(branches(app)).toContain("spike/webgl");
    });
  });

  describe("the dialogs", () => {
    const login = "Fix the login redirect";
    const worktreeOf = (branch: string) =>
      project("app")?.workspaces.find((one) => one.branch === branch);

    it("s asks before stopping a session, and stops it", async () => {
      await selectRow(login);
      await press("s");
      const stop = dialog(`Stop ${login}?`);
      await expect(stop).toBeDisplayed();
      await press("Enter");
      await untilSession(login, "exited");
    });

    it("r starts an ended session again", async () => {
      const id = session(login)?.id ?? "";
      await selectRow(login);
      await press("r");
      await until(() => launches(id).length >= 2, "the agent to start again");
      await until(() => session(login)?.status !== "exited", "the session to run again");
    });

    it("x removes an ended session at once", async () => {
      await selectRow(login);
      await press("s");
      await press("Enter");
      await untilSession(login, "exited");
      // x asks first while the window still shows it at work.
      await untilRowSays(login, /^(ended|exited)/);
      await press("x");
      await until(() => !session(login), "the session to be removed");
      expect(await noDialog()).toBe(true);
    });

    it("t asks which agent tidies a worktree's commits, and starts it there", async () => {
      const tree = worktreeOf("fix/login");
      if (!tree) throw new Error("no fix/login worktree");
      commit(tree.path, "wip one", { "login.txt": "one\n" });
      commit(tree.path, "wip two", { "login.txt": "two\n" });
      const before = new Set(sessionIds());
      await selectRow("fix/login");
      await press("t");
      const tidy = dialog("Tidy the commits on fix/login?");
      await expect(tidy).toBeDisplayed();
      await radio(tidy, "Claude Code").click();
      await button(tidy, "Tidy with Claude Code").click();
      await until(noDialog, "the dialog to close");
      const made = await until(
        () => sessionsNow().find((one) => !before.has(one.id)),
        "the tidying agent",
      );
      expect(made.harness).toBe("claude");
      expect(made.workspace).toBe(tree.id);
      const started = await until(() => launches(made.id)[0], "the agent to start");
      expect(started.cwd).toBe(realpathSync(tree.path));
    });

    it("u asks before moving a branch onto its base, and moves it", async () => {
      commit(app, "main moved on", { "main.txt": "new\n" });
      await selectRow("fix/login");
      await press("u");
      const move = dialog("Move fix/login onto main?");
      await expect(move).toBeDisplayed();
      // Refused while the tidying agent works there, in the daemon's words.
      await press("Enter");
      await expect(move).toHaveText(expect.stringContaining("is at work there"));
      expect(log(app, "fix/login")).not.toContain("main moved on");
      const tidying = sessionsNow().find(
        (one) => one.harness === "claude" && one.branch === "fix/login" && one.status === "running",
      );
      if (!tidying) throw new Error("no tidying agent");
      await finish(await until(() => launches(tidying.id)[0], "the tidying agent"));
      await until(() => session(tidying.title)?.status !== "running", "its turn to end");
      await button(move, "Move it").click();
      await until(() => log(app, "fix/login").includes("main moved on"), "fix/login to be on main");
      await until(noDialog, "the dialog to close");
      expect(log(app, "fix/login")).toEqual(expect.arrayContaining(["wip two", "wip one"]));
    });

    it("x on a worktree asks, says its changes, and removes it with them, keeping its branch", async () => {
      const tree = worktreeOf("spike/webgl");
      if (!tree) throw new Error("no spike/webgl worktree");
      write(tree.path, "draft.txt", "not committed\n");
      await selectRow("spike/webgl");
      await press("x");
      const remove = dialog("Remove ⎇ spike/webgl?");
      await expect(remove).toBeDisplayed();
      await expect(remove).toHaveText(
        expect.stringContaining("It has 1 file with changes not committed."),
      );
      await button(remove, "Remove with its changes").click();
      await until(noDialog, "the dialog to close");
      await until(() => !worktreeOf("spike/webgl"), "the worktree to go");
      expect(existsSync(tree.path)).toBe(false);
      expect(branches(app)).toContain("spike/webgl");
    });

    it("the palette runs every action it offers: anywhere, and on a project, a worktree and a session", async () => {
      /** Picks the palette's item `label` once `typed` finds it. */
      const run = async (typed: string, label: string) => {
        await find(typed);
        await option(palette(), label).click();
      };
      const opens = async (title: string) => {
        await expect(dialog(title)).toBeDisplayed();
        await press("Escape");
        await until(noDialog, `${title} to close`);
      };
      await pressOnWindow("⌘2");
      await run("add a project", "Add a project");
      await opens("Add a project");
      await run("new session", "Start a new session");
      await opens("New session");
      for (const screen of ["Sessions", "Needs you", "Tickets", "Settings"] as const) {
        await run(`go to ${screen}`, `Go to ${screen}`);
        expect(await shownScreen()).toBe(screen);
      }
      await run("fold the sidebar", "Fold the sidebar away");
      await expect($('nav[aria-label="Screens"]')).toHaveElementClass("mu-nav--folded");
      await run("fold the sidebar", "Fold the sidebar away");
      await expect($('nav[aria-label="Screens"]')).not.toHaveElementClass("mu-nav--folded");
      await run("every key", "Every key of this screen");
      await expect(help()).toBeDisplayed();
      await press("Escape");
      await until(noDialog, "the keys to close");

      await selectRow("app");
      await run("new worktree", "Make a new worktree for the project");
      await opens("New worktree");
      await selectRow("app");
      await run("stop tracking", "Stop tracking the project");
      await opens("Stop tracking app?");
      await selectRow("fix/login");
      await run("tidy", "Tidy the commits");
      await opens("Tidy the commits on fix/login?");
      await selectRow("fix/login");
      await run("move the branch", "Move the branch onto its base");
      await opens("Move fix/login onto main?");
      await selectRow("fix/login");
      await run("remove the worktree", "Remove the worktree");
      await opens("Remove ⎇ fix/login?");
      await selectRow("Palette agent");
      await run("rename", "Rename the session");
      await expect(field($("main"), "Session name")).toBeDisplayed();
      await press("Escape");
      await selectRow("Palette agent");
      await run("restart", "Restart the session");
      await until(() => session("Palette agent")?.status !== "exited", "it to run again");
    });

    it("x on a project asks before Muster stops tracking it, and leaves its folder", async () => {
      const root = project("lib")?.root ?? "";
      await selectRow("lib");
      await press("x");
      const forget = dialog("Stop tracking lib?");
      await expect(forget).toBeDisplayed();
      await button(forget, "Stop tracking").click();
      await until(noDialog, "the dialog to close");
      await until(() => !project("lib"), "lib to be forgotten");
      expect(existsSync(root)).toBe(true);
    });
  });
});

function sessionsNow() {
  return (status().projects ?? []).flatMap((one) =>
    one.workspaces.flatMap((workspace) => workspace.sessions),
  );
}

function sessionIds(): string[] {
  return sessionsNow().map((one) => one.id);
}
