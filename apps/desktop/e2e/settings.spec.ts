import { expect, type Page, test } from "@playwright/test";

// Settings, end to end over the mock daemon (boards SettingsGeneral,
// SettingsNotify, SettingsAgents, SettingsNaming, SettingsProject): each
// page in the states the boards draw, changed with the keys and the
// pointer, and what the daemon is sent. Plugins and Keys have files of
// their own.

interface Mock {
  sent: unknown[];
  heard: string[];
  publish(parts: Record<string, unknown>): void;
  settings: {
    setPreferences(change: Record<string, unknown>): void;
    editByHand(change: Record<string, unknown>): void;
    setProject(project: string, record: Record<string, unknown>): void;
  };
}

/** Runs `body` against the mock daemon in the page. */
function mock<T, A = undefined>(page: Page, body: (mock: Mock, arg: A) => T, arg?: A): Promise<T> {
  return page.evaluate(
    ([source, given]) => {
      const run = new Function("mock", "arg", `return (${source})(mock, arg);`);
      return run((window as unknown as { __muster: Mock }).__muster, given) as T;
    },
    [body.toString(), arg] as const,
  );
}

function sent(page: Page): Promise<unknown[]> {
  return mock(page, (daemon) => daemon.sent);
}

const KEP_SITE = "01JD0000000000000000000003";
const MUSTER = "01JD0000000000000000000001";
const LAMPHAUS = "01JD0000000000000000000002";

async function openSettings(page: Page, section?: string) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+,");
  await expect(page.getByRole("heading", { name: "Settings", level: 1 })).toBeVisible();
  if (section) {
    await page
      .getByRole("region", { name: "Settings sections" })
      .getByRole("button", { name: new RegExp(`^${section}`) })
      .click();
  }
}

/** A row of Settings by its key in the list: `general/colours`, `agents/agent.goose`. */
function row(page: Page, key: string) {
  return page.locator(`[data-row="${key}"]`);
}

/** A row that only says something, by its name. */
function still(page: Page, name: string) {
  return page.locator(".mu-setting").filter({ hasText: name });
}

function note(page: Page) {
  return page.locator(".mu-statusbar .mu-note");
}

test.describe("General", () => {
  test("shows Muster's look and layout, as its board does", async ({ page }) => {
    await openSettings(page);
    await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
    for (const name of ["theme", "session_lines", "panels", "colours", "side_by_side"]) {
      await expect(row(page, `general/${name}`)).toBeVisible();
    }
    await expect(page.getByText("list | session 60% | (changes / shell / plugin)")).toBeVisible();
    await expect(page.getByRole("main").getByText("Muster 0.4.1", { exact: true })).toBeVisible();
    await expect(page.getByText("~/.config/muster/settings.toml")).toBeVisible();
    // The status bar says what the keys do here.
    await expect(page.locator(".mu-statusbar")).toContainText("read the files again");
  });

  test("the theme is this window's own, and changes it at once", async ({ page }) => {
    await openSettings(page);
    await page.getByRole("group", { name: "Theme" }).getByRole("button", { name: "Light" }).click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await expect(note(page)).toHaveText("This window is light.");
    // ⏎ goes round, ← → step: here from light back to dark.
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    expect(await sent(page)).not.toContainEqual(
      expect.objectContaining({ ChangePreference: expect.anything() }),
    );
  });

  test("changes Muster's own settings with the keys, through the daemon", async ({ page }) => {
    await openSettings(page);
    await page.keyboard.press("ArrowDown");
    await expect(row(page, "general/session_lines")).toHaveClass(/is-selected/);
    await page.keyboard.press("ArrowRight");
    await expect(note(page)).toHaveText("Each session takes one line.");
    await expect(
      page
        .getByRole("group", { name: "Lines for each session" })
        .getByRole("button", { name: "One" }),
    ).toHaveAttribute("aria-pressed", "true");
    // Down to the width a session sits beside the list from, a step larger.
    await page.keyboard.press("j");
    await page.keyboard.press("j");
    await page.keyboard.press("ArrowDown");
    await expect(row(page, "general/side_by_side")).toHaveClass(/is-selected/);
    await page.keyboard.press("ArrowRight");
    await expect(note(page)).toHaveText(
      "A session sits beside the list in a window of 110 columns or more.",
    );
    await expect(page.getByRole("button", { name: "Session beside the list from" })).toHaveText(
      "110 columns",
    );
    expect(await sent(page)).toEqual(
      expect.arrayContaining([
        { ChangePreference: { SessionLines: "one" } },
        { ChangePreference: { SideBySideFrom: 110 } },
      ]),
    );
  });

  test("a select opens on ⏎ and chooses with the keys", async ({ page }) => {
    await openSettings(page);
    await row(page, "general/colours").click({ position: { x: 20, y: 10 } });
    await page.keyboard.press("Enter");
    const menu = page.getByRole("listbox", { name: "Colours" });
    await expect(menu).toBeVisible();
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Enter");
    await expect(menu).toBeHidden();
    await expect(page.getByRole("button", { name: "Colours" })).toHaveText("The terminal's own");
    await expect(note(page)).toHaveText("Muster draws in your terminal's colours.");
  });

  test("puts the panels back as at first, or says they are", async ({ page }) => {
    await openSettings(page);
    await page.getByRole("button", { name: "Put back as at first" }).click();
    await expect(note(page)).toHaveText("The panels are where they were at first.");
    await mock(page, (daemon) =>
      daemon.settings.setPreferences({ panels: "session | list", list_folded: true }),
    );
    await expect(page.getByText("session | list", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Put back as at first" }).click();
    await expect(note(page)).toHaveText("The panels are back where they were at first.");
    expect(await sent(page)).toEqual(
      expect.arrayContaining([
        { ChangePreference: { ListFolded: false } },
        { ChangePreference: { Panels: "list | session 60% | (changes / shell / plugin)" } },
      ]),
    );
  });

  test("says Muster's own settings do not read, and reads them again on r", async ({ page }) => {
    await openSettings(page);
    await mock(page, (daemon) =>
      daemon.settings.editByHand({
        problem: "keys.rename: d is show_changes's on the same screen",
      }),
    );
    await page.keyboard.press("r");
    await expect(note(page)).toHaveText("Read the settings again.");
    await expect(
      page.getByRole("status").filter({ hasText: "Muster's own settings do not read" }),
    ).toContainText("keys.rename: d is show_changes's on the same screen");
    expect(await sent(page)).toContainEqual("ReadPreferences");
  });
});

test.describe("Notifications", () => {
  test("turns notifications off, and their kinds with them", async ({ page }) => {
    await openSettings(page, "Notifications");
    await expect(page.getByText("Sent by Muster's helper")).toBeVisible();
    await page.getByRole("switch", { name: "Notifications" }).click();
    await expect(note(page)).toHaveText(
      "Nothing is told: the waiting count and Needs you still say it.",
    );
    await expect(page.getByRole("switch", { name: "Notifications" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    await expect(row(page, "notifications/tell.question")).toHaveAttribute("aria-disabled", "true");
    await expect(page.getByRole("button", { name: "Sound for questions" })).toBeDisabled();
    await expect(page.getByText("Off above, so Muster's helper sends none.")).toBeVisible();
  });

  test("hears each sound as it is chosen, and keeps the one chosen", async ({ page }) => {
    await openSettings(page, "Notifications");
    await page.getByRole("button", { name: "Sound for errors" }).click();
    const menu = page.getByRole("listbox", { name: "Sound for errors" });
    await expect(menu.getByRole("option", { name: "Basso" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(note(page)).toHaveText("Playing Blow");
    await page.keyboard.press("Enter");
    await expect(note(page)).toHaveText("Errors are told, with Blow.");
    await expect(page.getByRole("button", { name: "Sound for errors" })).toHaveText("Blow");
    expect(await mock(page, (daemon) => daemon.heard)).toEqual(["Blow"]);
    expect(await sent(page)).toContainEqual({
      ChangePreference: { Tell: { kind: "error", tell: { on: true, sound: "Blow" } } },
    });
  });

  test("steps a kind's sound with ← →, and turns it on or off with ⏎", async ({ page }) => {
    await openSettings(page, "Notifications");
    await page.keyboard.press("ArrowDown");
    await expect(row(page, "notifications/tell.question")).toHaveClass(/is-selected/);
    await page.keyboard.press("ArrowRight");
    await expect(note(page)).toHaveText("Questions are told, with Hero.");
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    // From the Mac's default to none, which is not played.
    await expect(note(page)).toHaveText("Finishes are told, with no sound.");
    await page.keyboard.press("Enter");
    await expect(note(page)).toHaveText("Finishes are not told.");
    expect(await mock(page, (daemon) => daemon.heard)).toEqual(["Hero"]);
  });

  test("says whether macOS allows them, and turns the notch off", async ({ page }) => {
    await openSettings(page, "Notifications");
    await expect(still(page, "Allowed in macOS")).toContainText("not yet");
    await mock(page, (daemon) => daemon.publish({ notifier: true }));
    await expect(still(page, "Allowed in macOS")).toContainText("✓ allowed");
    await page.getByRole("switch", { name: "Agents in the notch" }).click();
    await expect(note(page)).toHaveText("Muster leaves the notch alone.");
    expect(await sent(page)).toContainEqual({ ChangePreference: { Notch: false } });
  });
});

test.describe("Agents", () => {
  test("lists Muster's own, then the person's once their file is read again", async ({ page }) => {
    await openSettings(page, "Agents");
    for (const name of ["Claude Code", "Codex", "Gemini CLI", "OpenCode", "Copilot CLI"]) {
      await expect(page.locator(".mu-agentrow").filter({ hasText: name })).toBeVisible();
    }
    await expect(row(page, "agents/agent.gemini")).toContainText("names chats, not checked yet");
    await expect(page.getByText("None yet.")).toBeVisible();
    await mock(page, (daemon) =>
      daemon.settings.editByHand({
        agents: [
          {
            name: "goose",
            label: "Goose",
            mark: "•",
            colour: null,
            takes_prompt: true,
            asks: false,
          },
        ],
      }),
    );
    await page.getByRole("button", { name: "Read it again" }).click();
    await expect(row(page, "agents/agent.goose")).toContainText("added by you");
    await expect(row(page, "agents/agent.goose")).toContainText("takes a first prompt");
    await expect(
      page
        .getByRole("region", { name: "Settings sections" })
        .getByRole("button", { name: /^Agents/ }),
    ).toContainText("6");
  });

  test("says what of Muster's own settings does not read", async ({ page }) => {
    await openSettings(page, "Agents");
    await mock(page, (daemon) =>
      daemon.settings.setPreferences({
        problem:
          '[agents.goose] resume does not read: it must be a list of words, such as ["--resume"]',
      }),
    );
    await expect(page.getByRole("status")).toContainText("[agents.goose] resume does not read");
  });
});

test.describe("Naming chats", () => {
  test("names chats by the agent picked, with a model it lists or one typed", async ({ page }) => {
    await openSettings(page, "Naming chats");
    await expect(page.getByRole("button", { name: "Its model" })).toBeDisabled();
    await page.getByRole("button", { name: "Named by" }).click();
    await page.getByRole("option", { name: "Claude Code" }).click();
    await expect(note(page)).toHaveText("Chats are named by the agent shown.");
    await expect(page.getByRole("button", { name: "Named by" })).toHaveText("Claude Code");
    await page.getByRole("button", { name: "Its model" }).click();
    const models = page.getByRole("listbox", { name: "Its model" });
    await expect(models.getByRole("option", { name: "sonnet" })).toBeVisible();
    await models.getByRole("option", { name: "sonnet" }).click();
    await expect(note(page)).toHaveText("Chats are named with the model shown.");
    await expect(page.getByRole("button", { name: "Its model" })).toContainText("sonnet");
    // One typed, as the agent lists none.
    await page.getByRole("button", { name: "Its model" }).click();
    await page.getByRole("option", { name: "Type one…" }).click();
    const typed = page.getByRole("textbox", { name: "Its model, typed" });
    await expect(typed).toBeFocused();
    await typed.fill("claude-haiku-5-5");
    await typed.press("Enter");
    await expect(page.getByRole("button", { name: "Its model" })).toContainText("claude-haiku-5-5");
    expect(await sent(page)).toEqual(
      expect.arrayContaining([
        { ListModels: { agent: "claude" } },
        { ChangePreference: { NamedBy: { agent: "claude", model: "sonnet" } } },
        { ChangePreference: { NamedBy: { agent: "claude", model: "claude-haiku-5-5" } } },
      ]),
    );
  });

  test("asks for the agent first, and leaves chats their prompt once turned off", async ({
    page,
  }) => {
    await openSettings(page, "Naming chats");
    await row(page, "naming/naming_model").click({ position: { x: 30, y: 10 } });
    await page.keyboard.press("Enter");
    await expect(note(page)).toHaveText(
      "Pick the agent that names chats first: each uses its own model.",
    );
    await page.getByRole("switch", { name: "Chats named by an agent" }).click();
    await expect(note(page)).toHaveText("Chats keep the first line of their first prompt.");
    await expect(row(page, "naming/named_by")).toHaveAttribute("aria-disabled", "true");
    await expect(page.getByText("Claude Code and Codex.")).toBeVisible();
  });
});

test.describe("a project's page", () => {
  test("shows where its tickets come from, its fields and its branches", async ({ page }) => {
    await openSettings(page, "kep-site");
    await expect(page.getByRole("heading", { name: "kep-site" })).toBeVisible();
    await expect(page.getByText("It shares its tickets with kep-api")).toBeVisible();
    await expect(page.getByRole("button", { name: "Tickets from" })).toHaveText("Linear");
    await expect(row(page, `project:${KEP_SITE}/tickets_from`)).toContainText("Linear, team KEP");
    await expect(page.getByLabel("Team key, such as ENG")).toHaveValue("KEP");
    await expect(row(page, `project:${KEP_SITE}/muster_prefix`)).toContainText("from muster.toml");
    expect(await sent(page)).toContainEqual({ ProjectSettings: { project: KEP_SITE } });
  });

  test("turns only my tickets and branches with muster/ on and off", async ({ page }) => {
    await openSettings(page, "kep-site");
    await page.getByRole("switch", { name: "Only my tickets" }).click();
    await expect(note(page)).toHaveText(
      "Tickets for kep-site now lists only the tickets assigned to you.",
    );
    await page.getByRole("switch", { name: "Branches with muster/" }).click();
    await expect(note(page)).toHaveText(
      "New branches in kep-site go without muster/: issue-42-fix-the-login.",
    );
    await expect(row(page, `project:${KEP_SITE}/muster_prefix`)).not.toContainText(
      "from muster.toml",
    );
  });

  test("chooses where its tickets come from, asking for what the tracker needs", async ({
    page,
  }) => {
    await openSettings(page, "muster");
    await expect(row(page, `project:${MUSTER}/tickets_from`)).toContainText(
      "GitHub issues, as its git remote says",
    );
    // Linear needs a team: the cursor goes there, and ⏎ keeps it.
    await page.getByRole("button", { name: "Tickets from" }).click();
    await page.getByRole("option", { name: "Linear" }).click();
    await expect(note(page)).toHaveText("Linear needs its Team key, such as ENG.");
    const team = page.getByLabel("Team key, such as ENG");
    await expect(team).toBeFocused();
    await team.fill("ENG");
    await team.press("Enter");
    await expect(note(page)).toHaveText("muster's tickets now come from Linear, team ENG.");
    // Jira needs a site; once one project has it, another is offered it and kept at once.
    await page.getByRole("button", { name: "Tickets from" }).click();
    await page.getByRole("option", { name: "Jira" }).click();
    await expect(note(page)).toHaveText("Jira needs its Jira site, such as acme.atlassian.net.");
    const site = page.getByLabel("Jira site, such as acme.atlassian.net");
    await expect(site).toBeFocused();
    await site.fill("acme.atlassian.net");
    await site.press("Enter");
    await expect(note(page)).toHaveText(
      "muster's tickets now come from Jira at acme.atlassian.net.",
    );
    await page
      .getByRole("region", { name: "Settings sections" })
      .getByRole("button", { name: /^lamphaus/ })
      .click();
    await page.getByRole("button", { name: "Tickets from" }).click();
    await page.getByRole("option", { name: "Jira" }).click();
    await expect(note(page)).toHaveText(
      "lamphaus's tickets now come from Jira at acme.atlassian.net.",
    );
    expect(await sent(page)).toEqual(
      expect.arrayContaining([
        {
          ChangeSetting: {
            project: MUSTER,
            change: { TicketsFrom: { tracker: "linear", fields: { team: "ENG" } } },
          },
        },
        {
          ChangeSetting: {
            project: MUSTER,
            change: { TicketsFrom: { tracker: "jira", fields: { site: "acme.atlassian.net" } } },
          },
        },
        {
          ChangeSetting: {
            project: LAMPHAUS,
            change: { TicketsFrom: { tracker: "jira", fields: { site: "acme.atlassian.net" } } },
          },
        },
      ]),
    );
  });

  test("asks to choose where nothing tells, and says settings that do not read", async ({
    page,
  }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
    await mock(
      page,
      (daemon, project) => daemon.settings.setProject(project, { remote: null }),
      MUSTER,
    );
    await mock(
      page,
      (daemon, project) =>
        daemon.settings.setProject(project, {
          broken: "~/.config/muster/projects/kep-site.toml is not valid TOML: expected `=`",
        }),
      KEP_SITE,
    );
    await page.keyboard.press("Meta+,");
    const sections = page.getByRole("region", { name: "Settings sections" });
    await sections.getByRole("button", { name: /^muster/ }).click();
    await expect(page.getByRole("button", { name: "Tickets from" })).toHaveText("Choose…");
    await expect(row(page, `project:${MUSTER}/tickets_from`)).toContainText(
      "says nothing of where its tickets are",
    );
    await sections.getByRole("button", { name: /^kep-site/ }).click();
    await expect(page.getByRole("status")).toContainText("Its settings cannot be read");
    await expect(page.getByRole("status")).toContainText("is not valid TOML");
  });

  test("is taken out of Muster after asking, refused while it has worktrees or running sessions", async ({
    page,
  }) => {
    await openSettings(page, "muster");
    await expect(page.getByRole("button", { name: "Remove…" })).toBeDisabled();
    await expect(page.getByText("It has 2 worktrees, so it cannot be removed yet.")).toBeVisible();
    const sections = page.getByRole("region", { name: "Settings sections" });
    await sections.getByRole("button", { name: /^kep-api/ }).click();
    await page.getByRole("button", { name: "Remove…" }).click();
    await page
      .getByRole("dialog", { name: "Remove kep-api from Muster?" })
      .getByRole("button", { name: "Remove kep-api" })
      .click();
    await expect(note(page)).toHaveText("stop kep-api's 1 running session(s) first");
    await sections.getByRole("button", { name: /^kep-site/ }).click();
    await page.getByRole("button", { name: "Remove…" }).click();
    const dialog = page.getByRole("dialog", { name: "Remove kep-site from Muster?" });
    await expect(dialog).toContainText("The folder and its repository stay as they are.");
    await expect(dialog.getByRole("button", { name: "Keep it" })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await page.getByRole("button", { name: "Remove…" }).click();
    await dialog.getByRole("button", { name: "Remove kep-site" }).click();
    await expect(note(page)).toHaveText(
      "Muster no longer tracks the project; its folder is untouched.",
    );
    await expect(sections.getByRole("button", { name: /^kep-site/ })).toBeHidden();
  });
});
