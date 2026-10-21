import { expect, type Page, test } from "@playwright/test";

// The flows end to end, in WebKit over the mock daemon: the command palette
// and every key (design/screens/Palette.html), a new session
// (NewSession.html), adding a project (AddProject.html), the session
// dialogs (Dialogs.html), the first run (Welcome.html) and the daemon out of
// reach (Away.html). The mock is driven through `window.__muster`; what the
// screens of other branches would select is selected through
// `window.__musterUi`.

const id = (n: number) => `01JD${String(n).padStart(22, "0")}`;
const SKETCH = id(101); // Codex, asking, in ⎇ feature/desktop-app
const FLAKY = id(103); // Claude Code, working, in ⎇ fix/resize-under-load
const AGP = id(201); // OpenCode, working, in ⎇ muster/issue-219-agp-9-1 (changes not committed)
const RESIZE = id(12);
const AGP_TREE = id(22);
const MUSTER = id(1);

type Mock = {
  sent: unknown[];
  world: { projects: unknown[]; workspaces: unknown[]; sessions: unknown[]; preferences: unknown };
  flows: { unreadable: Record<string, string> };
  publish(parts: unknown): void;
  hold(command: string): void;
  release(command: string): void;
  drop(paths: string[]): void;
  setLink(link: unknown): void;
  setStatus(session: string, status: unknown): void;
  updateSession(session: string, fields: unknown): void;
  emitSession(session: string): void;
};

async function open(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("img", { name: /working, .* waiting on you/ })).toBeVisible();
}

const TITLES: Record<string, string> = { "needs-you": "Needs you", all: "Sessions" };

/** Selects `item` on `screen`, as that screen's list would, once the screen is shown. */
async function select(page: Page, screen: string, item: string) {
  await page.evaluate(
    ([screen, item]) => {
      const ui = (
        window as unknown as {
          __musterUi: { show(s: string): void; select(s: string, i: string): void };
        }
      ).__musterUi;
      ui.show(screen as string);
      ui.select(screen as string, item as string);
    },
    [screen, item],
  );
  await expect(page.getByRole("heading", { name: TITLES[screen], exact: true })).toBeVisible();
}

async function sent(page: Page, name: string): Promise<unknown[]> {
  return page.evaluate(
    (name) =>
      (window as unknown as { __muster: Mock }).__muster.sent.filter(
        (command) => typeof command === "object" && command !== null && name in command,
      ),
    name,
  );
}

const statusBar = (page: Page) => page.locator("footer.mu-statusbar");

test.describe("ARC-24 the command palette", () => {
  test("⌘K opens it on the field, groups what is selected first, and ⎋ closes it", async ({
    page,
  }) => {
    await open(page);
    await select(page, "needs-you", SKETCH);
    await page.keyboard.press("Meta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await expect(palette).toBeVisible();
    const field = palette.getByRole("combobox", { name: "Go to or do" });
    await expect(field).toBeFocused();
    await expect(field).toHaveAttribute(
      "placeholder",
      "Go to a session, ticket or setting, or do something",
    );
    const groups = palette.locator(".mu-palette-group");
    await expect(groups).toHaveText([
      "Actions on Sketch the Tauri shell",
      "Anywhere",
      "Sessions",
      "Settings",
    ]);
    await expect(palette.getByRole("option", { name: /Rename the session/ })).toContainText("R");
    await expect(palette.locator(".mu-palette-foot")).toContainText("every key");
    await page.keyboard.press("Escape");
    await expect(palette).toBeHidden();
  });

  test("a setting's page chosen in it opens that page", async ({ page }) => {
    await open(page);
    for (const [typed, title] of [
      ["naming", "Naming chats"],
      ["keys", "Keys"],
    ] as const) {
      await page.keyboard.press("Meta+k");
      const palette = page.getByRole("dialog", { name: "Command palette" });
      await palette.getByRole("combobox", { name: "Go to or do" }).fill(typed);
      await palette
        .locator(".mu-palette-group", { hasText: "Settings" })
        .locator("xpath=following-sibling::*[1]")
        .waitFor();
      await palette.getByRole("option", { name: new RegExp(`^${title}`) }).click();
      await expect(page.locator(".mu-settings-title")).toHaveText(title);
    }
  });

  test("a setting found by its words opens its page with it selected", async ({ page }) => {
    await open(page);
    await page.keyboard.press("Meta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await palette.getByRole("combobox", { name: "Go to or do" }).fill("reset");
    await palette.getByRole("option", { name: /^Put the panels back as at first/ }).click();
    await expect(page.locator(".mu-settings-title")).toHaveText("General");
    await expect(page.locator('[data-row="general/panels"]')).toHaveClass(/is-selected/);
  });

  test("typing narrows each group, lights the letters, and finds tickets", async ({ page }) => {
    await open(page);
    await page.keyboard.press("Meta+k");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await expect(palette.getByRole("combobox")).toBeFocused();
    await page.keyboard.type("res");
    await expect(palette.getByRole("option").first()).toHaveAttribute("aria-selected", "true");
    const resize = palette.getByRole("option", { name: /^finished Claude Code Resize settles/ });
    await expect(resize).toBeVisible();
    await expect(resize.locator("mark")).toHaveText("Res");
    await expect(palette.getByRole("option", { name: /#205/ })).toBeVisible();
    await expect(palette.getByRole("option", { name: /Flaky resize test/ })).toBeVisible();
    await page.keyboard.type("zzz");
    await expect(
      palette.getByText("No action, session, ticket or setting holds “reszzz”."),
    ).toBeVisible();
  });

  test("↓ and ⏎ do the lit one: going to a session selects it and shows its terminal", async ({
    page,
  }) => {
    await open(page);
    await page.keyboard.press("Meta+k");
    await page.keyboard.type("flaky");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "Command palette" })).toBeHidden();
    await expect(page.getByRole("heading", { name: "Sessions", exact: true })).toBeVisible();
    // The session is selected: R renames it, in place on the list.
    await page.keyboard.press("Shift+R");
    await expect(page.getByRole("textbox", { name: "Session name" })).toHaveValue(
      "Flaky resize test under load",
    );
  });

  test("an action runs as its key would", async ({ page }) => {
    await open(page);
    await select(page, "needs-you", SKETCH);
    await page.keyboard.press("Meta+k");
    await page.keyboard.type("stop");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowUp");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "Stop Sketch the Tauri shell?" })).toBeVisible();
  });

  test("h lists every key of the screen in view, narrows by keys or words, and ⏎ closes it", async ({
    page,
  }) => {
    await open(page);
    await page.keyboard.press("h");
    const help = page.getByRole("dialog", { name: "Every key" });
    await expect(help).toBeVisible();
    await expect(help.getByRole("heading")).toHaveText([
      "On the queue",
      "In a session",
      "Anywhere",
    ]);
    await page.keyboard.type("rename");
    await expect(help.getByRole("listitem")).toHaveText(["rename the session's chat⇧R"]);
    await page.keyboard.press("Enter");
    await expect(help).toBeHidden();
    await page.keyboard.press("Meta+3");
    await page.keyboard.press("?");
    await expect(help.getByRole("heading")).toHaveText([
      "On the tickets",
      "Choosing an agent",
      "Anywhere",
    ]);
    await page.keyboard.type("nothing like it");
    await expect(help.getByText("No key or what it does holds “nothing like it”.")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(help).toBeHidden();
  });

  test("keys are the person's: a rebound key does it and shows in the hints and help", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      const mock = (window as unknown as { __muster: Mock }).__muster;
      const preferences = {
        ...(mock.world.preferences as object),
        keys: { rename: ["ctrl-e"], help: ["?"] },
      };
      mock.publish({ preferences });
    });
    await select(page, "all", SKETCH);
    await page.keyboard.press("Shift+R");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.keyboard.press("Control+e");
    await expect(page.getByRole("textbox", { name: "Session name" })).toBeVisible();
    await page.keyboard.press("Escape");
    // The status bar says the person's key for every key, not Muster's.
    await expect(statusBar(page).locator(".mu-hint").last()).toHaveText("?every key");
    await page.keyboard.press("h");
    await expect(page.getByRole("dialog", { name: "Every key" })).toHaveCount(0);
    await page.keyboard.press("?");
    const help = page.getByRole("dialog", { name: "Every key" });
    await page.keyboard.type("rename");
    await expect(help.getByRole("listitem")).toHaveText(["rename the session's chat⌃E"]);
  });
});

test.describe("PRD-03 a new session", () => {
  test("n on an asking session starts in its worktree, the last agent first, a prompt naming it", async ({
    page,
  }) => {
    await open(page);
    await select(page, "needs-you", SKETCH);
    await page.keyboard.press("n");
    const dialog = page.getByRole("dialog", { name: "New session" });
    await expect(dialog).toBeVisible();
    const prompt = dialog.getByRole("textbox", { name: "First prompt" });
    await expect(prompt).toBeFocused();
    await expect(dialog.getByRole("button", { name: "Project" })).toContainText("muster");
    await expect(dialog.getByRole("button", { name: "A worktree" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect(dialog.getByRole("button", { name: "Worktree", exact: true })).toContainText(
      "⎇ feature/desktop-app",
    );
    const agents = dialog.getByRole("radio");
    await expect(agents.first()).toHaveAccessibleName(/Claude Code/);
    await expect(agents.first()).toHaveAttribute("aria-checked", "true");
    await expect(agents.first()).toContainText("last chosen");
    await expect(agents.last()).toContainText("A shell");
    await prompt.fill("Add a Not seen filter\nto the sidebar");
    await page.keyboard.press("Meta+Enter");
    await expect(dialog).toBeHidden();
    expect(await sent(page, "StartSession")).toEqual([
      {
        StartSession: {
          workspace: id(11),
          harness: "claude",
          title: null,
          size: { cols: 100, rows: 30 },
          prompt: "Add a Not seen filter\nto the sidebar",
        },
      },
    ]);
    await expect(page.getByRole("heading", { name: "Sessions", exact: true })).toBeVisible();
  });

  test("a new worktree on a branch, then a shell in it, which takes no prompt", async ({
    page,
  }) => {
    await open(page);
    await page.keyboard.press("n");
    const dialog = page.getByRole("dialog", { name: "New session" });
    await dialog.getByRole("button", { name: "A new worktree" }).click();
    await dialog.getByRole("textbox", { name: /Branch/ }).fill("spike/xterm-webgl");
    await dialog.getByRole("radio", { name: /A shell/ }).click();
    await expect(dialog.getByRole("textbox", { name: "First prompt" })).toBeDisabled();
    await dialog.getByRole("button", { name: "Start a shell" }).click();
    await expect(dialog).toBeHidden();
    const made = (await sent(page, "CreateWorktree")) as { CreateWorktree: unknown }[];
    expect(made).toEqual([
      { CreateWorktree: { project: MUSTER, branch: "spike/xterm-webgl", on: null } },
    ]);
    const started = (await sent(page, "StartSession")) as {
      StartSession: { harness: string; prompt: unknown };
    }[];
    expect(started[0]?.StartSession).toMatchObject({ harness: "shell", prompt: null });
    // The shell is the one chosen last now. ⇧Q leaves its terminal, which has the keys.
    await page.keyboard.press("Shift+Q");
    await page.keyboard.press("n");
    await expect(
      page.getByRole("dialog", { name: "New session" }).getByRole("radio").first(),
    ).toHaveAccessibleName(/A shell/);
  });

  test("the daemon's refusal stays in the dialog, and ←→ go round the agents", async ({ page }) => {
    await open(page);
    await page.keyboard.press("n");
    const dialog = page.getByRole("dialog", { name: "New session" });
    await dialog.getByRole("button", { name: "A new worktree" }).click();
    await dialog.getByRole("textbox", { name: /Branch/ }).fill("not a branch");
    await dialog.getByRole("radio", { name: /Claude Code/ }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(dialog.getByRole("radio", { name: /Codex/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(dialog.getByRole("button", { name: "Start Codex" })).toBeVisible();
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await expect(dialog.getByRole("radio", { name: /A shell/ })).toBeFocused();
    await dialog.getByRole("button", { name: "Start a shell" }).click();
    await expect(dialog.getByRole("alert")).toHaveText('"not a branch" is not a valid branch name');
    expect(await sent(page, "StartSession")).toEqual([]);
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("w makes a worktree; on a plain folder w says why not and n starts in the folder", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", MUSTER);
    await page.keyboard.press("w");
    const dialog = page.getByRole("dialog", { name: "New worktree" });
    await expect(dialog.getByRole("textbox", { name: /Branch/ })).toBeVisible();
    await dialog.getByRole("button", { name: "Make the worktree" }).click();
    await expect(dialog).toBeHidden();
    await expect(statusBar(page)).toContainText("Made the worktree.");
    expect(await sent(page, "CreateWorktree")).toEqual([
      { CreateWorktree: { project: MUSTER, branch: null, on: null } },
    ]);

    await page.evaluate(() =>
      (window as unknown as { __muster: Mock }).__muster.drop(["/Users/you/code/labs"]),
    );
    await expect(statusBar(page)).toContainText("Added labs.");
    await page.keyboard.press("w");
    await expect(statusBar(page)).toContainText(
      "labs is a plain folder, not a git repository, so it has no worktrees: press n for a session in it.",
    );
    await page.keyboard.press("n");
    const session = page.getByRole("dialog", { name: "New session" });
    await expect(session.getByText("In the folder itself, as it is no repository")).toBeVisible();
    await expect(session.getByRole("textbox", { name: /Branch/ })).toHaveCount(0);
  });
});

test.describe("PRD-01 adding a project", () => {
  test("the folder browser: typing chooses, ↓ another, ⇥ goes in, ⇧⇥ up, a dot shows hidden ones", async ({
    page,
  }) => {
    await open(page);
    await page.getByRole("button", { name: "Add a project" }).click();
    const dialog = page.getByRole("dialog", { name: "Add a project" });
    const field = dialog.getByRole("combobox", { name: "Folder" });
    await expect(field).toBeFocused();
    await expect(field).toHaveValue("~/");
    const folders = dialog.getByRole("listbox");
    await expect(folders.getByRole("option")).toHaveText([
      "code",
      "Desktop",
      "Documents",
      "Downloads",
      "mustergit",
    ]);
    await field.fill("~/code/la");
    await expect(folders.getByRole("option")).toHaveText([
      "labs",
      "lamphausgit",
      "lamphaus-apigit",
    ]);
    await expect(folders.getByRole("option", { name: /^labs/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(folders.locator("mark").first()).toHaveText("la");
    await page.keyboard.press("ArrowDown");
    await expect(dialog.getByRole("button", { name: "Add lamphaus" })).toBeVisible();
    await page.keyboard.press("Tab");
    await expect(field).toHaveValue("~/code/lamphaus/");
    await page.keyboard.press("Shift+Tab");
    await expect(field).toHaveValue("~/code/");
    await page.keyboard.press("Shift+Tab");
    await expect(field).toHaveValue("~/");
    await page.keyboard.press("Shift+Tab");
    await expect(field).toHaveValue("/Users/");
    await expect(folders.getByRole("option")).toHaveText(["you"]);
    await field.fill("~/code/.");
    await expect(folders.getByRole("option")).toHaveText([".cache"]);
    await expect(
      dialog.getByText(
        "⇥ fills in the chosen folder. Folders starting with a dot show once you type one.",
      ),
    ).toBeVisible();
  });

  test("⏎ adds the chosen folder and selects it; a refusal says why, in the field", async ({
    page,
  }) => {
    await open(page);
    await page.keyboard.press("a");
    const dialog = page.getByRole("dialog", { name: "Add a project" });
    const field = dialog.getByRole("combobox", { name: "Folder" });
    await field.fill("~/muster");
    await page.keyboard.press("Enter");
    await expect(dialog.getByText("/Users/you/muster is already a project")).toBeVisible();
    await field.fill("~/code/lamphaus-a");
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(statusBar(page)).toContainText("Added lamphaus-api.");
    expect(await sent(page, "AddProject")).toContainEqual({
      AddProject: { root: "/Users/you/code/lamphaus-api", name: null },
    });
    await expect(page.getByRole("navigation", { name: "Screens" })).toContainText("lamphaus-api");
  });

  test("a folder it cannot read, or one not there, is said, never shown empty", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      (window as unknown as { __muster: Mock }).__muster.flows.unreadable["/Users/you/Documents"] =
        "Operation not permitted (os error 1)";
    });
    await page.keyboard.press("a");
    const dialog = page.getByRole("dialog", { name: "Add a project" });
    const field = dialog.getByRole("combobox", { name: "Folder" });
    await field.fill("~/Documents/");
    await expect(dialog.getByRole("alert")).toHaveText(
      "Cannot read /Users/you/Documents: Operation not permitted (os error 1)",
    );
    await field.fill("~/nowhere/");
    await expect(dialog.getByRole("alert")).toContainText("No such file or directory");
    await field.fill("~/code/labs/");
    await expect(
      dialog.getByText("~/code/labs/ holds no folders. ⏎ adds the path as typed."),
    ).toHaveCount(0);
    await expect(dialog.getByRole("option", { name: "notes" })).toBeVisible();
  });

  test("a folder dropped on the window is added", async ({ page }) => {
    await open(page);
    await page.evaluate(() =>
      (window as unknown as { __muster: Mock }).__muster.drop(["/Users/you/code/lamphaus-api"]),
    );
    await expect(statusBar(page)).toContainText("Added lamphaus-api.");
    await page.evaluate(() =>
      (window as unknown as { __muster: Mock }).__muster.drop(["/Users/you/code/lamphaus"]),
    );
    await expect(statusBar(page)).toContainText("/Users/you/code/lamphaus is already a project");
  });
});

test.describe("PRD-09 the session dialogs", () => {
  test("s asks before stopping; x then removes the ended session at once; r restarts it", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", FLAKY);
    await page.keyboard.press("r");
    await expect(statusBar(page)).toContainText("Flaky resize test under load is still running.");
    await page.keyboard.press("s");
    const dialog = page.getByRole("dialog", { name: "Stop Flaky resize test under load?" });
    await expect(dialog).toContainText("r starts it again");
    await expect(dialog.getByRole("button", { name: "Stop session" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(statusBar(page)).toContainText("Stopping the session.");
    await page.keyboard.press("s");
    await expect(statusBar(page)).toContainText("That session has already ended.");
    await page.keyboard.press("r");
    await expect.poll(() => sent(page, "RestartSession")).toHaveLength(1);
    await page.keyboard.press("s");
    await page.keyboard.press("Enter");
    await expect(statusBar(page)).toContainText("Stopping the session.");
    await page.keyboard.press("x");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(statusBar(page)).toContainText("Removed the session.");
  });

  test("removing a worktree: its agents first, then its changes not committed, said before they go", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", AGP_TREE);
    await page.keyboard.press("x");
    const dialog = page.getByRole("dialog", { name: "Remove ⎇ muster/issue-219-agp-9-1?" });
    await expect(dialog).toContainText(
      "It has 4 files with changes not committed. Removing deletes its folder and those changes. The branch stays.",
    );
    await expect(dialog.getByRole("button", { name: "Keep it" })).toBeFocused();
    await dialog.getByRole("button", { name: "Remove with its changes" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      "stop the worktree's 1 running session(s) first",
    );
    await page.keyboard.press("Escape");
    await page.evaluate((agp) => {
      const mock = (window as unknown as { __muster: Mock }).__muster;
      mock.updateSession(agp, { ended_at: 1 });
      mock.setStatus(agp, { state: "exited", success: true });
    }, AGP);
    await select(page, "all", AGP_TREE);
    await page.keyboard.press("x");
    await dialog.getByRole("button", { name: "Remove with its changes" }).click();
    await expect(dialog).toBeHidden();
    await expect(statusBar(page)).toContainText(
      "Removed the worktree; ⎇ muster/issue-219-agp-9-1 stays.",
    );
    expect(await sent(page, "RemoveWorktree")).toContainEqual({
      RemoveWorktree: { workspace: AGP_TREE, force: true },
    });
  });

  test("x on a project asks before Muster stops tracking it, and says the daemon's refusal", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", MUSTER);
    await page.keyboard.press("x");
    const dialog = page.getByRole("dialog", { name: "Stop tracking muster?" });
    await dialog.getByRole("button", { name: "Stop tracking" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      "muster has 2 worktree(s); remove them first",
    );
    await select(page, "all", id(10));
    await page.keyboard.press("Escape");
    await page.keyboard.press("x");
    await expect(statusBar(page)).toContainText(
      "That is the project's own checkout, which Muster never removes; select the project to stop tracking it.",
    );
  });

  test("t asks which agent tidies, ⇥ going round them; the daemon's refusal stays", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", RESIZE);
    await page.keyboard.press("t");
    const dialog = page.getByRole("dialog", { name: "Tidy the commits on fix/resize-under-load?" });
    await expect(dialog).toContainText("3 commits since origin/main.");
    await expect(dialog.getByRole("radio")).toHaveCount(5);
    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "Tidy with Codex" })).toBeVisible();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Shift+Tab");
    await expect(dialog.getByRole("button", { name: "Tidy with Copilot CLI" })).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(dialog.getByRole("alert")).toHaveText(
      "Flaky resize test under load is at work there: tidy the commits once it has stopped",
    );
    expect(await sent(page, "TidyCommits")).toEqual([
      { TidyCommits: { workspace: RESIZE, harness: "copilot", size: { cols: 100, rows: 30 } } },
    ]);
  });

  test("u asks before moving a branch, refused while an agent works there, then says where it went", async ({
    page,
  }) => {
    await open(page);
    await select(page, "all", id(21));
    await page.keyboard.press("u");
    const dialog = page.getByRole("dialog", {
      name: "Move muster/issue-212-gradle-cache onto origin/main?",
    });
    await expect(dialog).toContainText(
      "Its 3 commits are replayed on origin/main's newest; if one would conflict, nothing changes. What is there is kept first, so the move can be put back.",
    );
    await expect(dialog.getByRole("button", { name: "Move it" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(dialog.getByRole("alert")).toHaveText(
      "Share the Gradle cache across worktrees is at work there: move the branch once it has stopped",
    );
    await page.evaluate((gemini) => {
      const mock = (window as unknown as { __muster: Mock }).__muster;
      mock.updateSession(gemini, { ended_at: 1 });
      mock.setStatus(gemini, { state: "exited", success: true });
    }, id(200));
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(statusBar(page)).toContainText(
      "muster/issue-212-gradle-cache is on origin/main now, its 3 commits replayed there; z on #1 in d puts it back.",
    );
  });

  test("on a branch stacked on one that merged, u says so; on one that has not, it moves onto it", async ({
    page,
  }) => {
    await open(page);
    const stack = (merged: boolean) =>
      page.evaluate(
        ([resize, desktop, merged]) => {
          const mock = (window as unknown as { __muster: Mock }).__muster;
          const world = mock.world.workspaces as {
            id: string;
            stacked_on: unknown;
            pull_request: { state: string } | null;
          }[];
          const parent = world.find((w) => w.id === resize);
          const child = world.find((w) => w.id === desktop);
          if (parent?.pull_request)
            parent.pull_request = { ...parent.pull_request, state: merged ? "merged" : "open" };
          if (child) child.stacked_on = { branch: "fix/resize-under-load", at: "a1b2c3d" };
          mock.publish({ workspaces: [parent, child] });
        },
        [RESIZE, id(11), merged] as const,
      );
    await stack(true);
    await select(page, "all", id(11));
    await page.keyboard.press("u");
    const merged = page.getByRole("dialog", { name: "Move feature/desktop-app onto main?" });
    await expect(merged).toContainText(
      "The branch it was stacked on, fix/resize-under-load, merged as #88.",
    );
    await page.keyboard.press("Escape");
    await stack(false);
    await page.keyboard.press("u");
    const stacked = page.getByRole("dialog", {
      name: "Move feature/desktop-app onto fix/resize-under-load?",
    });
    await expect(stacked).toContainText("It is stacked on fix/resize-under-load.");
  });

  test("keys that need a worktree say what to select, in the TUI's words", async ({ page }) => {
    await open(page);
    await select(page, "all", MUSTER);
    await page.keyboard.press("t");
    await expect(statusBar(page)).toContainText(
      "Select a worktree, or a session in one, to tidy its commits.",
    );
    await page.keyboard.press("u");
    await expect(statusBar(page)).toContainText(
      "Select a worktree, or a session in one, to move its branch onto its base.",
    );
    await page.keyboard.press("s");
    await expect(statusBar(page)).toContainText("Select a session to stop.");
    // Tidying is on the list of all sessions only, as in the TUI.
    await select(page, "needs-you", SKETCH);
    await page.keyboard.press("t");
    await expect(statusBar(page)).not.toContainText("Select");
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });
});

test.describe("PRD-01 the first run and the daemon out of reach", () => {
  test("with no projects, Kep welcomes, a adds one, and a dropped folder does too", async ({
    page,
  }) => {
    await open(page);
    await page.evaluate(() => {
      const mock = (window as unknown as { __muster: Mock }).__muster;
      mock.world.projects = [];
      mock.world.workspaces = [];
      mock.world.sessions = [];
      mock.publish({ reset: true, preferences: mock.world.preferences });
    });
    await expect(page.getByRole("heading", { name: "Add a project to start" })).toBeVisible();
    await expect(page.getByRole("img", { name: "Kep, Muster's sheepdog" })).toBeVisible();
    await expect(page.getByText("Or drop a folder on this window.")).toBeVisible();
    // The key, then what it does.
    await expect(statusBar(page)).toContainText(/\ba\s*add a project/);
    await page.keyboard.press("a");
    await expect(page.getByRole("dialog", { name: "Add a project" })).toBeVisible();
    await page.keyboard.press("Escape");
    await page.evaluate(() =>
      (window as unknown as { __muster: Mock }).__muster.drop(["/Users/you/muster"]),
    );
    await expect(statusBar(page)).toContainText("Added muster.");
    await expect(page.getByRole("heading", { name: "Add a project to start" })).toBeHidden();
  });

  test("out of reach: the banner says so and how long, the window stands still, nothing asks", async ({
    page,
  }) => {
    await open(page);
    await select(page, "needs-you", SKETCH);
    await page.keyboard.press("Meta+k");
    await page.evaluate(() => {
      const mock = (window as unknown as { __muster: Mock }).__muster;
      mock.setLink({ state: "down", reason: "no daemon answers", since: Date.now() - 14_000 });
    });
    await expect(page.getByRole("dialog")).toHaveCount(0);
    const banner = page.getByRole("alert");
    await expect(banner).toContainText("Muster's daemon is out of reach.");
    await expect(banner).toContainText("Starting it again and trying every half second");
    await expect(banner).toContainText("What you see is as it was.");
    await expect(page.locator("main.app-screen")).toHaveAttribute("inert", "");
    await expect(page.getByRole("button", { name: "Waiting for the daemon" })).toBeDisabled();
    await page.keyboard.press("n");
    await page.keyboard.press("Meta+k");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.evaluate(() => {
      (window as unknown as { __muster: Mock }).__muster.setLink({
        state: "up",
        version: "0.4.1",
        protocol: 44,
      });
    });
    await expect(page.getByRole("alert")).toHaveCount(0);
    await page.keyboard.press("n");
    await expect(page.getByRole("dialog", { name: "New session" })).toBeVisible();
  });
});
