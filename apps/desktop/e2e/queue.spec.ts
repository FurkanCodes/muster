import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// Needs you end to end in WebKit, over the mock daemon (window.__muster):
// the queue in the sidebar, the selected session beside it.

declare global {
  interface Window {
    __muster: MockBridge;
    __musterTerminals: Record<string, () => string>;
  }
}

const codex = "01JD0000000000000000000101";
const gemini = "01JD0000000000000000000200";

const queue = (page: Page) => page.getByRole("listbox");
const option = (page: Page, name: string) => queue(page).getByRole("option", { name, exact: true });
const statusBar = (page: Page) => page.getByRole("contentinfo");
const terminal = (page: Page) => page.getByLabel("Terminal", { exact: true });

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(option(page, "Sketch the Tauri shell")).toBeVisible();
});

test("ARC-24 Needs you lists by project; under each, who asks with what they ask, then finishes not seen; never the agents at work", async ({
  page,
}) => {
  await expect(page.getByRole("button", { name: /^Needs you/ })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  const projects = queue(page).locator(".queue-project");
  await expect(projects).toHaveText(["muster", "lamphaus"]);
  await expect(queue(page).getByRole("option")).toHaveCount(4);

  const asking = option(page, "Sketch the Tauri shell");
  await expect(asking).toHaveAttribute("aria-selected", "true");
  await expect(asking.locator(".mu-list-item-meta")).toHaveText("wants to run cargo build");

  const finish = option(page, "Resize settles once on release");
  await expect(finish.locator(".mu-list-item-meta")).toBeVisible();
  await expect(finish).toContainText("⎇ fix/resize-under-load");
  await expect(finish).toContainText("#88");
  await expect(finish.getByRole("img", { name: "not seen" })).toBeVisible();

  // At work, it needs nobody yet: All lists it.
  await expect(option(page, "Upgrade AGP to 9.1")).toHaveCount(0);

  // Beside it the session: its head, its banner, its terminal, and nothing under it.
  const pane = page.getByRole("region", { name: "Sketch the Tauri shell", exact: true });
  await expect(pane.getByRole("heading", { name: "Sketch the Tauri shell" })).toBeVisible();
  await expect(pane).toContainText("⎇ feature/desktop-app in muster");
  await expect(pane.locator(".mu-banner")).toContainText("Needs you: wants to run cargo build");
  await expect(terminal(page)).toBeVisible();
  await expect(page.getByRole("region", { name: "This turn" })).toHaveCount(0);
  // What it runs on and uses, at the status bar's end.
  await expect(statusBar(page)).toContainText("Codex · 182k tokens · 212 MB, 3% CPU");
  await expect(statusBar(page)).toContainText("answer");
  await expect(statusBar(page)).toContainText("put back");
});

test("ARC-24 every reason to ask shows in the row and the banner, and the banner goes as it stops asking", async ({
  page,
}) => {
  // With nothing from its hooks, the row says why the agent waits.
  const said = {
    permission: "wants your permission",
    question: "asks you something",
    startup: "asks before it starts",
    action: "wants you to act",
  };
  for (const [reason, text] of Object.entries(said)) {
    await page.evaluate(
      ({ id, reason }) => {
        window.__muster.updateSession(id, { activity: null });
        window.__muster.setStatus(id, {
          state: "needs_you",
          reason: reason as "permission" | "question" | "startup" | "action",
        });
      },
      { id: codex, reason },
    );
    await expect(option(page, "Sketch the Tauri shell").locator(".mu-list-item-meta")).toHaveText(
      text,
    );
    await expect(page.locator(".session-banner")).toContainText(text);
  }
  await page.evaluate(
    (id) => window.__muster.setStatus(id, { state: "exited", success: false }, true),
    codex,
  );
  await expect(page.locator(".session-banner")).toHaveCount(0);
  await expect(
    option(page, "Sketch the Tauri shell").getByRole("img", { name: "exited with an error" }),
  ).toBeVisible();
});

test("ARC-24 a session that newly asks flashes once; what was there as the window opened does not", async ({
  page,
}) => {
  await expect(option(page, "Sketch the Tauri shell")).not.toHaveClass(/mu-flash/);
  await page.evaluate(() =>
    window.__muster.setStatus("01JD0000000000000000000201", {
      state: "needs_you",
      reason: "question",
    }),
  );
  await expect(option(page, "Upgrade AGP to 9.1")).toHaveClass(/mu-flash--waiting/);
});

test("ARC-24 All clear says how many work; the one selected stays until another is", async ({
  page,
}) => {
  await page.evaluate(() => {
    const mock = window.__muster;
    for (const session of [...mock.world.sessions]) {
      if (session.status.state === "needs_you") mock.setStatus(session.id, { state: "running" });
      else if (session.unseen) mock.setStatus(session.id, { state: "finished" });
    }
  });
  await expect(page.getByRole("heading", { name: "All clear" })).toBeVisible();
  await expect(
    page.getByText("6 agents at work. Muster will show them here when they need you."),
  ).toBeVisible();
  await expect(queue(page).getByRole("option")).toHaveText([/Sketch the Tauri/]);
  await page.evaluate(() => {
    const mock = window.__muster;
    for (const session of [...mock.world.sessions]) mock.removeSession(session.id);
  });
  await expect(
    page.getByText("No agent is at work. Muster will show them here when they need you."),
  ).toBeVisible();
  await expect(terminal(page)).toHaveCount(0);
  await expect(page.getByText("The session you select shows here, live.")).toBeVisible();
});

test("ARC-24 by project, the most urgent first; under each, what asks, then what finished unseen", async ({
  page,
}) => {
  await expect(queue(page).getByRole("option")).toHaveText([
    /Sketch the Tauri/,
    /Resize settles/,
    /Share the Gradle/,
    /Explain the release/,
  ]);
});

test("ARC-24 ↑↓ move, ⏎ gives the live terminal the keys, ⇧Q gives them back", async ({ page }) => {
  await page.keyboard.press("ArrowDown");
  await expect(option(page, "Resize settles once on release")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("heading", { name: "Resize settles once on release" })).toBeVisible();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(terminal(page)).toHaveClass(/has-keys/);
  await expect(option(page, "Sketch the Tauri shell")).toHaveClass(/is-resting/);
  await expect(statusBar(page)).toContainText(
    "Typing in Sketch the Tauri shell. Every key goes to Codex.",
  );
  await expect(statusBar(page)).toContainText("give the keys back");
  await page.keyboard.type("yes");
  await expect
    .poll(() => page.evaluate((id) => window.__musterTerminals[id]?.(), codex))
    .toContain("yes");
  // The list heard none of it.
  await expect(option(page, "Sketch the Tauri shell")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Shift+Q");
  await expect(terminal(page)).not.toHaveClass(/has-keys/);
  // The finish opened on the way down was seen, and has left the queue.
  await page.keyboard.press("j");
  await expect(option(page, "Share the Gradle cache across worktrees")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  // A click on the terminal gives it the keys too.
  await terminal(page).click();
  await expect(terminal(page)).toHaveClass(/has-keys/);
});

test("ARC-24 opening a finish clears its news, and it keeps its place until another is selected", async ({
  page,
}) => {
  await option(page, "Resize settles once on release").click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          window.__muster.world.sessions.find((s) => s.title === "Resize settles once on release")
            ?.unseen,
      ),
    )
    .toBe(false);
  await expect(option(page, "Resize settles once on release")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.keyboard.press("ArrowDown");
  await expect(option(page, "Share the Gradle cache across worktrees")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(option(page, "Resize settles once on release")).toHaveCount(0);
  // One that leaves gives its place to the next.
  await page.evaluate(() => {
    const id = window.__muster.world.sessions.find(
      (s) => s.title === "Share the Gradle cache across worktrees",
    )?.id;
    if (id) window.__muster.removeSession(id);
  });
  await expect(option(page, "Explain the release script")).toHaveAttribute("aria-selected", "true");
});

test("ARC-24 l goes to the session that needed the person last", async ({ page }) => {
  await page.keyboard.press("l");
  await expect(statusBar(page)).toContainText("No session has needed you since Muster started.");
  await page.evaluate(() =>
    window.__muster.setStatus("01JD0000000000000000000201", {
      state: "needs_you",
      reason: "question",
    }),
  );
  await page.keyboard.press("l");
  await expect(option(page, "Upgrade AGP to 9.1")).toHaveAttribute("aria-selected", "true");
  await page.evaluate(() =>
    window.__muster.setStatus("01JD0000000000000000000201", { state: "finished" }),
  );
  await page.keyboard.press("k");
  await page.keyboard.press("l");
  await expect(page.getByRole("tree", { name: "Projects and sessions" })).toBeVisible();
});

test("ARC-24 R renames in place, ⎋ gives up, and the more menu offers the same with its keys", async ({
  page,
}) => {
  await page.keyboard.press("Shift+R");
  const name = page.getByRole("textbox", { name: "Session name" });
  await expect(name).toBeFocused();
  await name.fill("A name of my own");
  await name.press("Enter");
  await expect(option(page, "A name of my own")).toBeVisible();
  await expect(page.getByRole("heading", { name: "A name of my own" })).toBeVisible();
  await expect(statusBar(page)).toContainText("Renamed.");
  await page.keyboard.press("Shift+R");
  await name.fill("Not this");
  await name.press("Escape");
  await expect(name).toHaveCount(0);
  await expect(option(page, "A name of my own")).toBeVisible();

  await page.getByRole("button", { name: "More", exact: true }).click();
  const menu = page.getByRole("menu", { name: "A name of my own" });
  // A session's menu is the same whichever list it was chosen in (app/Detail.tsx).
  await expect(menu.getByRole("menuitem")).toHaveText([
    "Full screenf",
    "Pin to the right⇧P",
    "Rename⇧R",
    "Run the checksc",
    "Review the changev",
    "Open a draft pull requesto",
    "Pull request in the browserb",
    "Closex",
  ]);
  // Its keys are its own once it has the focus, which it takes as it opens.
  await expect(menu).toBeFocused();
  await page.keyboard.press("Shift+R");
  await expect(menu).toHaveCount(0);
  await expect(name).toBeFocused();
});

test("ARC-24 b opens the session's pull request in the browser, and says when none is known", async ({
  page,
}) => {
  await page.keyboard.press("b");
  await expect
    .poll(() => page.evaluate(() => window.__muster.openedUrls))
    .toEqual(["https://github.com/FurkanCodes/muster/pull/91"]);
  await option(page, "Explain the release script").click();
  await page.keyboard.press("b");
  await expect(statusBar(page)).toContainText(
    "No pull request from this branch is known; o opens one.",
  );
  // On a finish too.
  await option(page, "Resize settles once on release").click();
  await page.keyboard.press("b");
  await expect
    .poll(() => page.evaluate(() => window.__muster.openedUrls))
    .toEqual([
      "https://github.com/FurkanCodes/muster/pull/91",
      "https://github.com/FurkanCodes/muster/pull/88",
    ]);
});

test("PRD-09 x asks first, ⎋ keeps it, and Close session stops it and takes it off the list", async ({
  page,
}) => {
  await page.keyboard.press("x");
  const dialog = page.getByRole("dialog", { name: "Close Sketch the Tauri shell?" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Keep it" })).toBeFocused();
  await page.keyboard.press("j");
  // The list behind is hidden from a screen reader while the dialog asks.
  await expect(
    page
      .getByRole("listbox", { includeHidden: true })
      .getByRole("option", { name: "Sketch the Tauri shell", exact: true, includeHidden: true }),
  ).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await page.keyboard.press("x");
  await dialog.getByRole("button", { name: "Close session" }).click();
  await expect(option(page, "Sketch the Tauri shell")).toHaveCount(0);
  await expect(option(page, "Share the Gradle cache across worktrees")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(statusBar(page)).toContainText("Closed the session.");
  const sent = await page.evaluate(() =>
    window.__muster.sent.filter(
      (c) => typeof c === "object" && ("StopSession" in c || "RemoveSession" in c),
    ),
  );
  expect(sent).toEqual([
    { StopSession: { session: codex } },
    { RemoveSession: { session: codex } },
  ]);
});

test("PRD-14 z opens the newest turn's putting back in the changes, never while its agent works; d the changes", async ({
  page,
}) => {
  const changes = page.getByRole("button", { name: "Changes", exact: true });
  await page.evaluate((id) => window.__muster.setStatus(id, { state: "running" }), codex);
  await page.keyboard.press("z");
  await expect(statusBar(page)).toContainText("Wait for its turn to end.");
  await expect(changes).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("d");
  await expect(changes).toHaveAttribute("aria-pressed", "true");
  await changes.click();
  await expect(changes).toHaveAttribute("aria-pressed", "false");
  await page.evaluate((id) => window.__muster.setStatus(id, { state: "finished" }), codex);
  await option(page, "Sketch the Tauri shell").click();
  await page.keyboard.press("z");
  // Behind the put-back dialog, hidden from a screen reader while it asks.
  await expect(
    page.getByRole("button", { name: "Changes", exact: true, includeHidden: true }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("ARC-24 the queue keeps its last state while the daemon is out of reach", async ({ page }) => {
  await page.evaluate(() =>
    window.__muster.setLink({ state: "down", reason: "the daemon stopped", since: 0 }),
  );
  // The window's banner says it, with the daemon's reason (src/flows/Away.tsx).
  await expect(queue(page).page().getByRole("alert")).toContainText(
    "Muster's daemon is out of reach.",
  );
  await expect(queue(page).page().getByRole("alert")).toContainText("the daemon stopped");
  await expect(option(page, "Sketch the Tauri shell")).toHaveAttribute("aria-selected", "true");
});

test("ARC-24 at its least, the sidebar stays and the session keeps 520 wide", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await expect(page.getByRole("navigation", { name: "Sessions" })).toBeVisible();
  const pane = page.getByRole("region", { name: "Sketch the Tauri shell", exact: true });
  expect((await pane.boundingBox())?.width).toBeGreaterThanOrEqual(520);
  await expect(page.getByRole("button", { name: "Changes", exact: true })).toBeInViewport();
  await expect(page.getByRole("button", { name: "More", exact: true })).toBeInViewport();
});

test("ARC-24 an asking row says what it asks, and leaves Needs you as it stops", async ({
  page,
}) => {
  const row = option(page, "Share the Gradle cache across worktrees");
  await expect(row.locator(".mu-list-item-meta")).toHaveText("asks which cache folder to use");
  await page.evaluate((id) => window.__muster.setStatus(id, { state: "running" }), gemini);
  // At work again, it needs nobody: All lists it.
  await expect(row).toHaveCount(0);
});
