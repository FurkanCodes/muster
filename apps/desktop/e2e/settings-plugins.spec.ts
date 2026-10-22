import { expect, type Page, test } from "@playwright/test";

// Settings › Plugins, end to end over the mock daemon (boards
// SettingsPlugins and PluginApprove): the list and the details beside it,
// turning one on and off with the terminal UI's refusals, adding one from
// an address or a folder after seeing everything it runs, updating one,
// removing one after asking, and giving up a fetch that takes its time.

interface Mock {
  sent: unknown[];
  hold(command: string): void;
  release(command: string): void;
}

function mock<T>(page: Page, body: (mock: Mock) => T): Promise<T> {
  return page.evaluate((source) => {
    const run = new Function("mock", `return (${source})(mock);`);
    return run((window as unknown as { __muster: Mock }).__muster) as T;
  }, body.toString());
}

function sent(page: Page): Promise<unknown[]> {
  return mock(page, (daemon) => daemon.sent);
}

async function openPlugins(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+,");
  await page
    .getByRole("region", { name: "Settings sections" })
    .getByRole("button", { name: /^Plugins/ })
    .click();
  await expect(page.getByRole("heading", { name: "Plugins", level: 2 })).toBeVisible();
}

function plugin(page: Page, id: string) {
  return page.locator(`[data-row="plugins/plugin.${id}"]`);
}

function note(page: Page) {
  return page.locator(".mu-statusbar .mu-note");
}

test("lists Muster's own then the person's, each with what it brings or why it is off", async ({
  page,
}) => {
  await openPlugins(page);
  await expect(plugin(page, "github")).toContainText("GitHub issues");
  await expect(plugin(page, "github")).toContainText("built in");
  await expect(plugin(page, "github")).toContainText("tickets: GitHub");
  await expect(plugin(page, "notify-webhook")).toContainText("hook: needs you, finished, failed");
  await expect(plugin(page, "session-graph")).toContainText("panel: graph (⇧G)");
  await expect(plugin(page, "youtrack")).toContainText("Off: changed since you approved it");
  await expect(page.getByRole("switch", { name: "YouTrack tickets" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
  // Showing the screen reads the plugins again.
  expect(await sent(page)).toContainEqual({ Plugin: "List" });
});

test("shows everything the selected plugin runs, needs and is given", async ({ page }) => {
  await openPlugins(page);
  await plugin(page, "youtrack")
    .getByRole("button", { name: /YouTrack tickets/ })
    .click();
  const details = page.getByRole("region", { name: /YouTrack tickets/ });
  await expect(details).toContainText("Off until you approve it again:");
  await expect(details).toContainText("its manifest changed since you approved it");
  await expect(details).toContainText(
    "https://github.com/acme/muster-youtrack, main, commit 3f9a2c1",
  );
  await expect(details).toContainText("./youtrack --serve");
  await expect(details).toContainText("jq ✗ not found");
  await expect(details).toContainText("brew install jq");
  await expect(details).toContainText("YOUTRACK_SITE, and the secret YOUTRACK_TOKEN");
  await expect(details).toContainText("Updated 2 days ago from the command line");
  await expect(details.getByRole("button", { name: "Review and approve" })).toBeVisible();
  await expect(page.locator(".mu-statusbar")).toContainText("turn on or off");
});

test("turns one off and on with space, refusing one that cannot run with why", async ({ page }) => {
  await openPlugins(page);
  await plugin(page, "session-graph")
    .getByRole("button", { name: /Session graph/ })
    .click();
  await page.keyboard.press(" ");
  await expect(note(page)).toHaveText("Turned session-graph off.");
  await expect(page.getByRole("switch", { name: "Session graph" })).toHaveAttribute(
    "aria-checked",
    "false",
  );
  await page.getByRole("switch", { name: "YouTrack tickets" }).click();
  await expect(note(page)).toHaveText(
    "youtrack cannot be turned on: its manifest changed since you approved it: update it, or link it again, to see what it runs now and approve it.",
  );
  expect(await sent(page)).toContainEqual({
    Plugin: { SetEnabled: { id: "session-graph", on: false, by: "ui" } },
  });
  expect(await sent(page)).not.toContainEqual({
    Plugin: { SetEnabled: { id: "youtrack", on: true, by: "ui" } },
  });
});

test("⏎ on one that changed shows what it runs now, and approving it updates it", async ({
  page,
}) => {
  await openPlugins(page);
  await plugin(page, "youtrack")
    .getByRole("button", { name: /YouTrack tickets/ })
    .click();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Update YouTrack tickets?" });
  await expect(dialog).toContainText("Nothing of it runs until you say yes.");
  await expect(dialog).toContainText("0.2.1");
  await expect(dialog).toContainText("0.2.0 from https://github.com/acme/muster-youtrack");
  await expect(dialog).toContainText("a manifest that is not this one");
  await expect(dialog).toContainText("It needs programs that are not on PATH");
  await page.keyboard.press("y");
  await expect(dialog).toBeHidden();
  await expect(note(page)).toHaveText("Updated youtrack to 0.2.1.");
  await expect(plugin(page, "youtrack")).not.toContainText("Off:");
});

test("adds one from an address after showing everything it runs", async ({ page }) => {
  await openPlugins(page);
  await page.keyboard.press("a");
  const dialog = page.getByRole("dialog", { name: "Add a plugin" });
  const address = dialog.getByLabel("A git repository's address, or a folder's path");
  await expect(address).toBeFocused();
  await address.fill("https://github.com/acme/nowhere");
  await address.press("Enter");
  await expect(dialog.getByRole("alert")).toHaveText(
    "could not fetch the plugin: git: repository 'https://github.com/acme/nowhere' not found",
  );
  await expect(address).toHaveValue("https://github.com/acme/nowhere");
  await address.fill("~/plugins/mine");
  await address.press("Enter");
  await expect(dialog.getByRole("alert")).toContainText("Type the folder's whole path, from /");
  await address.fill("https://github.com/acme/muster-standup");
  await address.press("Enter");
  const preview = page.getByRole("dialog", { name: "Install Stand-up notes?" });
  await expect(preview).toContainText(
    "./note {project}, when an agent finishes, only project muster; at most 10 seconds each time",
  );
  await expect(preview).toContainText("A hook only hears.");
  await expect(preview).toContainText("plugins/standup/5e0a91c/");
  await preview.getByRole("button", { name: "Install and turn on" }).click();
  await expect(note(page)).toHaveText("Installed standup and turned it on.");
  await expect(plugin(page, "standup")).toHaveClass(/is-selected/);
  await expect(plugin(page, "standup")).toContainText("hook: finished");
});

test("links a folder, and says nothing was added when the person says no", async ({ page }) => {
  await openPlugins(page);
  await page.getByRole("button", { name: "Add a plugin", exact: true }).click();
  const address = page.getByLabel("A git repository's address, or a folder's path");
  await address.fill("/Users/you/muster/examples/plugins/tickets-from-file");
  await expect(page.getByLabel("Branch or tag")).toBeDisabled();
  await address.press("Enter");
  const preview = page.getByRole("dialog", { name: "Link Tickets from a file?" });
  await expect(preview).toContainText("linked where it is and read from there");
  await page.keyboard.press("n");
  await expect(preview).toBeHidden();
  await expect(note(page)).toHaveText("Nothing was added.");
});

test("a fetch that takes its time can be given up", async ({ page }) => {
  await openPlugins(page);
  await mock(page, (daemon) => daemon.hold("Plugin"));
  await page.keyboard.press("a");
  const address = page.getByLabel("A git repository's address, or a folder's path");
  await address.fill("https://github.com/acme/muster-standup");
  await address.press("Enter");
  const waiting = page.getByRole("dialog", { name: "Add a plugin" });
  await expect(waiting.getByRole("status", { name: "Fetching the plugin" })).toHaveAttribute(
    "aria-busy",
    "true",
  );
  await page.keyboard.press("Escape");
  await expect(waiting).toBeHidden();
  await expect(note(page)).toHaveText(
    "Stopped waiting for https://github.com/acme/muster-standup.",
  );
  // The answer that comes after is dropped.
  await mock(page, (daemon) => daemon.release("Plugin"));
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("removes one after asking; Muster's own are refused updating and removing", async ({
  page,
}) => {
  await openPlugins(page);
  await plugin(page, "github")
    .getByRole("button", { name: /GitHub issues/ })
    .click();
  await page.keyboard.press("x");
  await expect(note(page)).toHaveText(
    "github is Muster's own: it cannot be removed, only turned off.",
  );
  await page.keyboard.press("u");
  await expect(note(page)).toHaveText("github is Muster's own: it updates with Muster.");
  await plugin(page, "notify-webhook")
    .getByRole("button", { name: /Notify by webhook/ })
    .click();
  await page.keyboard.press("x");
  const dialog = page.getByRole("dialog", { name: "Remove notify-webhook?" });
  await expect(dialog).toContainText(
    "Muster forgets it; its folder /Users/you/muster/examples/plugins/notify-webhook stays as it is.",
  );
  await dialog.getByRole("button", { name: "Remove notify-webhook" }).click();
  await expect(note(page)).toHaveText("Removed notify-webhook.");
  await expect(plugin(page, "notify-webhook")).toHaveCount(0);
  // The selection goes to adding one.
  await expect(page.locator('[data-row="plugins/add_plugin"]')).toHaveClass(/is-selected/);
});
