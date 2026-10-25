import { expect, type Page, test } from "@playwright/test";

// Settings › Keys, end to end over the mock daemon (board SettingsKeys):
// every action under where it works, a key set by pressing it, refused as
// the terminal UI refuses it, one action's keys and every key put back,
// the keys narrowed by typing, and a key the person set that does nothing.

interface Mock {
  sent: unknown[];
  settings: { setPreferences(change: Record<string, unknown>): void };
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

async function openKeys(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+,");
  await page
    .getByRole("region", { name: "Settings sections" })
    .getByRole("button", { name: /^Keys/ })
    .click();
  await expect(page.getByRole("heading", { name: "Keys", level: 2 })).toBeVisible();
}

function key(page: Page, action: string) {
  return page.locator(`[data-row="keys/key.${action}"]`);
}

function note(page: Page) {
  return page.locator(".mu-statusbar .mu-note");
}

test("lists every action under where it works, with its keys as set", async ({ page }) => {
  await openKeys(page);
  for (const group of [
    "This window",
    "Anywhere",
    "Sessions and Needs you",
    "Tickets",
    "Settings",
    "In a session",
  ]) {
    await expect(page.getByRole("heading", { name: group, exact: true, level: 3 })).toBeVisible();
  }
  await expect(key(page, "rename")).toContainText("Rename the session's chat");
  await expect(key(page, "rename").locator(".mu-kbd")).toHaveText(["⇧R"]);
  await expect(key(page, "leave_session").locator(".mu-kbd")).toHaveText(["⇧Q", "⌃]"]);
  await expect(page.getByRole("heading", { name: "Session graph's panels" })).toBeVisible();
  await expect(key(page, "plugin.session-graph.graph")).toContainText("its suggestion");
});

test("sets a key by pressing it, refusing one another action has on the same screen", async ({
  page,
}) => {
  await openKeys(page);
  await key(page, "new_session").click();
  await expect(key(page, "new_session")).toHaveClass(/is-waiting/);
  await expect(key(page, "new_session")).toContainText("Press a key…");
  await expect(note(page)).toHaveText(
    "Press the key for: A new session in the worktree, or a project's checkout.",
  );
  await page.keyboard.press("d");
  await expect(page.getByRole("alert")).toHaveText(
    "d already does “What the worktree or session has changed, and its turns” on this screen. Press another key.",
  );
  await expect(key(page, "new_session")).toHaveClass(/is-waiting/);
  await page.keyboard.press("Shift+N");
  await expect(note(page)).toHaveText("The key is set.");
  await expect(key(page, "new_session").locator(".mu-kbd")).toHaveText(["⇧N"]);
  await expect(key(page, "new_session")).toContainText("yours");
  expect(await sent(page)).toContainEqual({
    ChangePreference: { Keys: { action: "new_session", keys: ["N"] } },
  });
  expect(await sent(page)).not.toContainEqual({
    ChangePreference: { Keys: { action: "new_session", keys: ["d"] } },
  });
});

test("refuses keys every screen keeps, ⌘'s, and typing inside a terminal", async ({ page }) => {
  await openKeys(page);
  await key(page, "leave_session").click();
  await page.keyboard.press("x");
  await expect(page.getByRole("alert")).toContainText(
    "x would reach the program as typing. A key inside a terminal needs ⌃ or ⌥",
  );
  await page.keyboard.press("ArrowUp");
  await expect(page.getByRole("alert")).toHaveText(
    "↑ is a key that moves about, the same on every screen. Press another key.",
  );
  await page.keyboard.press("Meta+k");
  await expect(page.getByRole("alert")).toHaveText(
    "⌘ keys are this window's own. Press a key with ⌃ or ⌥, or none.",
  );
  // The palette stays shut: the key was the row's.
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(key(page, "leave_session")).not.toHaveClass(/is-waiting/);
  await expect(note(page)).toHaveText("Its keys stay as they are.");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Alt+q");
  await expect(note(page)).toHaveText("The key is set.");
  await expect(key(page, "leave_session").locator(".mu-kbd")).toHaveText(["⌥Q"]);
});

test("⌫ puts an action's keys back, ⇧R every key", async ({ page }) => {
  await openKeys(page);
  await mock(page, (daemon) =>
    daemon.settings.setPreferences({ keys: { rename: ["f2"], close: ["X"] } }),
  );
  await expect(key(page, "rename").locator(".mu-kbd")).toHaveText(["F2"]);
  await key(page, "rename").click();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Backspace");
  await expect(note(page)).toHaveText("Its keys are back as Muster has them.");
  await expect(key(page, "rename").locator(".mu-kbd")).toHaveText(["⇧R"]);
  await page.keyboard.press("Shift+R");
  await expect(note(page)).toHaveText("Every key is back as Muster has it.");
  await expect(key(page, "close").locator(".mu-kbd")).toHaveText(["x"]);
  expect(await sent(page)).toEqual(
    expect.arrayContaining([
      { ChangePreference: { Keys: { action: "rename", keys: null } } },
      { ChangePreference: "AllKeysBack" },
    ]),
  );
});

test("narrows the keys to what is typed, by words or by key", async ({ page }) => {
  await openKeys(page);
  const find = page.getByRole("textbox", { name: "Find an action or a key" });
  await find.fill("pull request");
  await expect(key(page, "open_pull_request")).toBeVisible();
  await expect(key(page, "pull_request_in_browser")).toBeVisible();
  await expect(key(page, "rename")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "This window" })).toHaveCount(0);
  await find.fill("put the files back");
  await expect(key(page, "put_back")).toBeVisible();
  await find.press("Enter");
  await expect(key(page, "put_back")).toHaveClass(/is-selected/);
  // By its key too.
  await find.fill("z");
  await expect(key(page, "put_back")).toBeVisible();
  await expect(key(page, "zoom")).toBeVisible();
  await find.fill("nothing does this");
  await expect(page.getByText("No key does “nothing does this”.")).toBeVisible();
});

test("says which of the person's keys do nothing", async ({ page }) => {
  await openKeys(page);
  await mock(page, (daemon) =>
    daemon.settings.setPreferences({ keys: { "plugin.gone.panel": ["Y"] } }),
  );
  await expect(
    page.getByText(
      "keys.plugin.gone.panel is not something a key does in Muster: kept, for a plugin's action of that name, and doing nothing meanwhile.",
    ),
  ).toBeVisible();
});

test("the keys set here are the ones the screens answer", async ({ page }) => {
  await openKeys(page);
  // read_settings on F5: r no longer reads the settings again; F5 does.
  await key(page, "read_settings").click();
  await page.keyboard.press("F5");
  await expect(note(page)).toHaveText("The key is set.");
  await page.keyboard.press("F5");
  await expect(note(page)).toHaveText("Read the settings again.");
  await page
    .getByRole("region", { name: "Settings sections" })
    .getByRole("button", { name: /^General/ })
    .click();
  await expect(page.locator(".mu-statusbar")).toContainText("F5read the files again");
});

test("a key is set from its own button too, for one who tabs to it", async ({ page }) => {
  await openKeys(page);
  const set = page.getByRole("button", {
    name: /^Set the key for Rename the session's chat, now ⇧R/,
  });
  await set.focus();
  await page.keyboard.press("Enter");
  await expect(key(page, "rename")).toHaveClass(/is-waiting/);
  await page.keyboard.press("F2");
  await expect(note(page)).toHaveText("The key is set.");
  await expect(key(page, "rename").locator(".mu-kbd")).toHaveText(["F2"]);
});
