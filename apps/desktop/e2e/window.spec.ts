import { expect, test } from "@playwright/test";

// The window's frame, against the mock daemon. Each screen's own end-to-end
// tests sit beside this file, one per branch.

test("the window shows the flock, the sidebar and, under All, the projects", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.getByRole("img", { name: "4 working, 2 waiting on you, 2 done" }),
  ).toBeVisible();
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await expect(sidebar).toBeVisible();
  await sidebar.getByRole("button", { name: /^All/ }).click();
  for (const project of ["muster", "lamphaus", "kep-site", "kep-api"]) {
    await expect(sidebar.getByRole("treeitem", { name: project, exact: true })).toBeVisible();
  }
});

test("keys go between screens", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+3");
  await expect(page.getByRole("heading", { name: "Tickets" })).toBeVisible();
  await page.keyboard.press("Meta+,");
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Meta+2");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
});

test("the window follows the daemon as a session comes to need you", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("img", { name: "Something asks for you" })).toBeVisible();
  await page.evaluate(() => {
    const mock = (
      window as unknown as { __muster: { setStatus: (id: string, status: unknown) => void } }
    ).__muster;
    mock.setStatus("01JD0000000000000000000100", { state: "needs_you", reason: "question" });
  });
  await expect(
    page.getByRole("img", { name: "3 working, 3 waiting on you, 2 done" }),
  ).toBeVisible();
});

test("the sidebar goes away and comes back, by key or by its button", async ({ page }) => {
  await page.goto("/");
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await expect(sidebar).toBeVisible();
  await page.keyboard.press("Meta+\\");
  await expect(sidebar).toBeHidden();
  await page.keyboard.press("Meta+\\");
  await expect(sidebar).toBeVisible();
  await page.getByRole("button", { name: "Hide the sidebar" }).click();
  await expect(sidebar).toBeHidden();
  // The session's head makes room for the window's buttons and brings it back.
  await page.getByRole("button", { name: "Show the sidebar" }).click();
  await expect(sidebar).toBeVisible();
});

test("a session's terminal streams its program's output and takes typing", async ({ page }) => {
  await page.goto("/");
  const codex = "01JD0000000000000000000101";
  const shown = () =>
    page.evaluate(
      (session) =>
        (
          window as unknown as { __musterTerminals?: Record<string, () => string> }
        ).__musterTerminals?.[session]?.() ?? "",
      codex,
    );
  await expect.poll(shown).toContain("Allow Codex to run this command?");
  await page.getByLabel("Terminal", { exact: true }).click();
  await page.keyboard.type("yes");
  await expect.poll(shown).toContain("yes");
});

test("an open terminal redraws as the Mac turns light or dark, with the theme following it", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.addInitScript(() => localStorage.setItem("muster.theme", "system"));
  await page.goto("/");
  const codex = "01JD0000000000000000000101";
  /** The background the terminal draws on, and the theme's own for it now. */
  const drawn = () =>
    page.evaluate((session) => {
      const shown =
        (
          window as unknown as { __musterBackgrounds?: Record<string, () => string> }
        ).__musterBackgrounds?.[session]?.() ?? "";
      const wanted = getComputedStyle(document.documentElement)
        .getPropertyValue("--surface-term")
        .trim();
      return { shown, wanted };
    }, codex);
  await expect.poll(async () => (await drawn()).shown).not.toBe("");
  const dark = await drawn();
  expect(dark.shown).toBe(dark.wanted);
  await page.emulateMedia({ colorScheme: "light" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect.poll(async () => (await drawn()).shown).not.toBe(dark.shown);
  const light = await drawn();
  expect(light.shown).toBe(light.wanted);
});
