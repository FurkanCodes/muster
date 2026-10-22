import { expect, type Page, test } from "@playwright/test";

// The pointer's ways, against the mock daemon: a click in the list gives the
// keys to the list, a right-click opens a line's menu where the pointer is,
// the window opens no browser menu, and the session's head leads on.

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Sessions" });
const queue = (page: Page) => page.getByRole("listbox", { name: "Needs you sessions" });

/** What has the focus: its role and name, as a person's screen reader would say it. */
const focused = (page: Page) =>
  page.evaluate(() => {
    const element = document.activeElement;
    return `${element?.getAttribute("role") ?? element?.tagName}: ${element?.getAttribute("aria-label") ?? ""}`;
  });

test("ARC-24 a click on a line gives the keys to the list, so the arrows leave nothing lit behind", async ({
  page,
}) => {
  await page.goto("/");
  await queue(page)
    .getByRole("option", { name: /^Share the Gradle/ })
    .click();
  expect(await focused(page)).toBe("listbox: Needs you sessions");
  await page.keyboard.press("ArrowDown");
  await expect(
    queue(page).getByRole("option", { name: "Explain the release script" }),
  ).toHaveAttribute("aria-selected", "true");
  // The list keeps the focus: no line holds it, so none can ring.
  expect(await focused(page)).toBe("listbox: Needs you sessions");
});

test("ARC-24 a right-click on a session's line selects it and opens its menu at the pointer", async ({
  page,
}) => {
  await page.goto("/");
  const line = queue(page).getByRole("option", { name: "Sketch the Tauri shell" });
  await line.click({ button: "right", position: { x: 120, y: 20 } });
  await expect(line).toHaveAttribute("aria-selected", "true");
  const menu = page.getByRole("menu", { name: "Sketch the Tauri shell" });
  await expect(menu).toBeVisible();
  const box = await menu.boundingBox();
  const at = await line.boundingBox();
  expect(Math.abs((box?.x ?? 0) - ((at?.x ?? 0) + 120))).toBeLessThan(2);
  expect(Math.abs((box?.y ?? 0) - ((at?.y ?? 0) + 20))).toBeLessThan(2);
  // Its keys are the ⋯ menu's: opened by the pointer it holds the focus with
  // no item lit, as a Mac's does; ↓ moves, ⎋ closes and gives the keys back to the list.
  await expect(menu).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem").first()).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem", { name: /^Rename/ })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  expect(await focused(page)).toBe("listbox: Needs you sessions");
});

test("ARC-24 a right-click in All opens a session's menu, and a project's line only selects it", async ({
  page,
}) => {
  await page.goto("/");
  await sidebar(page).getByRole("button", { name: /^All/ }).click();
  const tree = sidebar(page).getByRole("tree");
  await tree
    .getByRole("treeitem", { name: /^Explain the release script/ })
    .click({ button: "right" });
  await expect(page.getByRole("menu", { name: "Explain the release script" })).toBeVisible();
  await page.keyboard.press("Escape");
  await tree.getByRole("treeitem", { name: "lamphaus", exact: true }).click({ button: "right" });
  await expect(tree.getByRole("treeitem", { name: "lamphaus", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("menu")).toHaveCount(0);
});

test("ARC-24 the window opens no browser menu, except over text one can copy", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".app-head .app-head-title")).toBeVisible();
  await expect(page.locator(".xterm")).toBeVisible();
  const prevented = (selector: string) =>
    page.evaluate((at) => {
      const target = document.querySelector(at);
      const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      target?.dispatchEvent(event);
      return event.defaultPrevented;
    }, selector);
  expect(await prevented("footer.mu-statusbar")).toBe(true);
  expect(await prevented(".app-head")).toBe(true);
  expect(await prevented(".xterm")).toBe(false);
});

test("ARC-24 the session's head leads on: its changes, its project's page", async ({ page }) => {
  await page.goto("/");
  await queue(page).getByRole("option", { name: "Sketch the Tauri shell" }).click();
  const head = page.locator(".app-head");
  await head.getByRole("button", { name: "+300 −6" }).click();
  await expect(page.getByRole("button", { name: "Changes", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await head.getByRole("button", { name: /^⎇ feature\/desktop-app in muster/ }).click();
  await expect(page.getByPlaceholder("What should the agent do?")).toBeVisible();
});

test("ARC-24 with the sidebar away, every screen keeps the way back to it", async ({ page }) => {
  await page.goto("/");
  await page.keyboard.press("Meta+\\");
  for (const [key, screen] of [
    ["Meta+3", "Tickets"],
    ["Meta+4", "Settings"],
    ["Meta+1", "All"],
    ["Meta+2", "Needs you"],
  ] as const) {
    await page.keyboard.press(key);
    await expect(page.getByRole("button", { name: "Show the sidebar" }), screen).toBeVisible();
  }
  await page.keyboard.press("Meta+3");
  await page.getByRole("button", { name: "Show the sidebar" }).click();
  await expect(sidebar(page)).toBeVisible();
});

test("ARC-24 the sidebar's edge is dragged to resize it, kept, and put back by a double-click", async ({
  page,
}) => {
  await page.goto("/");
  const edge = page.getByRole("separator", { name: "Sidebar width" });
  const width = async () => (await sidebar(page).boundingBox())?.width ?? 0;
  expect(await width()).toBe(288);
  const box = await edge.boundingBox();
  const x = (box?.x ?? 0) + (box?.width ?? 0) / 2;
  const y = (box?.y ?? 0) + 200;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 60, y, { steps: 4 });
  await page.mouse.up();
  expect(await width()).toBe(348);
  await expect(edge).toHaveAttribute("aria-valuenow", "348");
  // Kept: the window opens again at the width it was left at.
  await page.reload();
  await expect(sidebar(page)).toBeVisible();
  expect(await width()).toBe(348);
  // Never narrower or wider than its bounds.
  const again = await edge.boundingBox();
  const from = (again?.x ?? 0) + (again?.width ?? 0) / 2;
  await page.mouse.move(from, y);
  await page.mouse.down();
  await page.mouse.move(from - 400, y, { steps: 4 });
  await page.mouse.up();
  expect(await width()).toBe(220);
  await edge.dblclick();
  expect(await width()).toBe(288);
});
