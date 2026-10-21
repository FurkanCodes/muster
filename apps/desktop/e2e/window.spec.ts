import { expect, test } from "@playwright/test";

// The window's frame, against the mock daemon. Each screen's own end-to-end
// tests sit beside this file, one per branch.

test("the window shows the flock, the screens and the projects", async ({ page }) => {
  await page.goto("/");
  await expect(
    page.getByRole("img", { name: "4 working, 2 waiting on you, 2 done" }),
  ).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Screens" })).toBeVisible();
  for (const project of ["muster", "lamphaus", "kep-site", "kep-api"]) {
    await expect(page.getByRole("button", { name: new RegExp(`^${project}`) })).toBeVisible();
  }
});

test("keys go between screens", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+3");
  await expect(page.getByRole("heading", { name: "Tickets" })).toBeVisible();
  await page.keyboard.press("Meta+,");
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await page.keyboard.press("Meta+1");
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

test("the sidebar folds and unfolds", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Projects")).toBeVisible();
  await page.keyboard.press("Meta+\\");
  await expect(page.getByText("Projects")).toBeHidden();
  await page.keyboard.press("Meta+\\");
  await expect(page.getByText("Projects")).toBeVisible();
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
