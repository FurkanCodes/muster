import { expect, type Page, test } from "@playwright/test";

// What a session's program is told of its terminal's size, against the mock
// daemon (its bridge keeps every size it is told, `sized`). A program is
// told only sizes its terminal is drawn at, each once: a program told a
// size it is not drawn at draws its screen for it, and what it drew stays
// behind once it is told the real one.

interface Sized {
  session: string;
  cols: number;
  rows: number;
}

type Mock = {
  sized: Sized[];
  world: { sessions: { id: string; title: string }[] };
  setStatus: (id: string, status: unknown) => void;
};

const sized = (page: Page) =>
  page.evaluate(() => [...(window as unknown as { __muster: Mock }).__muster.sized]);

const idOf = (page: Page, title: string) =>
  page.evaluate(
    (wanted) =>
      (window as unknown as { __muster: Mock }).__muster.world.sessions.find(
        (session) => session.title === wanted,
      )?.id ?? "",
    title,
  );

test("ARC-24 a terminal out of sight, or not yet measured, tells its program nothing", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator(".xterm")).toBeVisible();
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await sidebar.getByRole("option", { name: /^Share the Gradle/ }).click();
  await page.keyboard.press("Meta+\\");
  await page.keyboard.press("Meta+\\");
  // A project's page puts the session's terminal out of sight (0 by 0).
  await page.keyboard.press("Meta+1");
  await sidebar.getByRole("treeitem", { name: "muster", exact: true }).click();
  await expect(page.getByPlaceholder("What should the agent do?")).toBeVisible();
  await sidebar.getByRole("treeitem", { name: /^Split view/ }).click();
  await expect(page.locator(".xterm").first()).toBeVisible();
  await page.waitForTimeout(300);

  const told = await sized(page);
  expect(told.length).toBeGreaterThan(0);
  // A 1440 by 900 window draws no terminal anywhere near xterm's own 80 by 24.
  for (const size of told) {
    expect(size.cols, JSON.stringify(size)).toBeGreaterThan(100);
    expect(size.rows, JSON.stringify(size)).toBeGreaterThan(24);
  }
});

test("ARC-24 a session coming to ask keeps its terminal's size: the banner floats over it", async ({
  page,
}) => {
  await page.goto("/");
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await page.keyboard.press("Meta+1");
  await sidebar.getByRole("treeitem", { name: /^Explain the release script/ }).click();
  await expect(page.locator(".xterm").first()).toBeVisible();
  await page.waitForTimeout(300);
  const id = await idOf(page, "Explain the release script");
  const before = (await sized(page)).filter((size) => size.session === id).length;
  expect(before).toBeGreaterThan(0);

  await page.evaluate(
    (session) =>
      (window as unknown as { __muster: Mock }).__muster.setStatus(session, {
        state: "needs_you",
        reason: "question",
      }),
    id,
  );
  await expect(page.locator(".session-banner")).toBeVisible();
  await page.waitForTimeout(300);
  const after = (await sized(page)).filter((size) => size.session === id).length;
  expect(after).toBe(before);
});
