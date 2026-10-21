import { expect, type Page, test } from "@playwright/test";

// The terminal UI's one tab model in WebKit, with the real terminal (PRD
// §6, §6a): Tab and ⇧Tab go round Sessions, Needs you, Tickets, Settings
// and never into a terminal; ⏎ or a click gives a terminal the keys, ⇧Q
// takes them back; the status bar says where the keys are for exactly as
// long as it is so.

const h1 = (page: Page, name: string) => page.getByRole("heading", { name, level: 1, exact: true });
const terminal = (page: Page) => page.getByLabel("Terminal", { exact: true });
const statusBar = (page: Page) => page.getByRole("contentinfo");
const typing = (page: Page) => statusBar(page).locator("[data-typing]");
const inTerminal = (page: Page) =>
  page.evaluate(() => document.activeElement?.classList.contains("xterm-helper-textarea") ?? false);

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("option", { name: "Sketch the Tauri shell" })).toBeVisible();
});

test("ARC-24 Tab and ⇧Tab go round the tabs and never into a terminal", async ({ page }) => {
  await expect(h1(page, "Needs you")).toBeVisible();
  for (const next of ["Tickets", "Settings", "Sessions", "Needs you"]) {
    await page.keyboard.press("Tab");
    await expect(h1(page, next)).toBeVisible();
    expect(await inTerminal(page)).toBe(false);
  }
  await page.keyboard.press("Shift+Tab");
  await expect(h1(page, "Sessions")).toBeVisible();
  await expect(terminal(page)).not.toHaveClass(/has-keys/);
  await expect(typing(page)).toHaveCount(0);
  // The terminal's input is out of Tab's way altogether.
  await expect(page.locator(".xterm-helper-textarea").first()).toHaveAttribute("tabindex", "-1");
});

test("ARC-24 the status bar says a terminal has the keys while it does, and not a moment after", async ({
  page,
}) => {
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toHaveClass(/has-keys/);
  expect(await inTerminal(page)).toBe(true);
  await expect(typing(page)).toHaveText(
    "Typing in Sketch the Tauri shell. Every key goes to Codex.",
  );
  await page.keyboard.press("Shift+Q");
  await expect(terminal(page)).not.toHaveClass(/has-keys/);
  await expect(typing(page)).toHaveCount(0);
  await expect(statusBar(page)).not.toContainText("Typing in");
  expect(await inTerminal(page)).toBe(false);
  // Back with Muster, Tab is a tab again, and ↓ moves the list, not the agent.
  await page.keyboard.press("ArrowDown");
  await expect(
    page.getByRole("option", { name: "Share the Gradle cache across worktrees" }),
  ).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Tab");
  await expect(h1(page, "Tickets")).toBeVisible();
  await expect(statusBar(page)).not.toContainText("Typing in");
});

test("ARC-24 a click on the terminal gives it the keys, and says so", async ({ page }) => {
  await terminal(page).click();
  await expect(terminal(page)).toHaveClass(/has-keys/);
  await expect(typing(page)).toContainText("Typing in Sketch the Tauri shell.");
  await page.getByRole("option", { name: "Resize settles once on release" }).click();
  await expect(typing(page)).toHaveCount(0);
});
