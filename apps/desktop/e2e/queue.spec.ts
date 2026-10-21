import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// Needs you, Not seen and Working end to end in WebKit, over the mock
// daemon (window.__muster), in each state the Triage and Working boards draw.

declare global {
  interface Window {
    __muster: MockBridge;
    __musterTerminals: Record<string, () => string>;
  }
}

const codex = "01JD0000000000000000000101";
const gemini = "01JD0000000000000000000200";
const desktop = "01JD0000000000000000000011";

const queue = (page: Page) => page.getByRole("listbox");
const option = (page: Page, name: string) => queue(page).getByRole("option", { name, exact: true });
const statusBar = (page: Page) => page.getByRole("contentinfo");
const terminal = (page: Page) => page.getByLabel("Terminal", { exact: true });

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(option(page, "Sketch the Tauri shell")).toBeVisible();
});

test("ARC-24 Needs you lists who asks with their question, then finishes not seen, then agents at work", async ({
  page,
}) => {
  await expect(page.getByRole("heading", { name: "Needs you", level: 1 })).toBeVisible();
  await expect(page.getByText("Oldest first. Opening one clears it.")).toBeVisible();
  const sections = queue(page).locator(".mu-section");
  await expect(sections).toHaveText(["Asking you", "Finished, not seen", "Working"]);
  await expect(queue(page).getByRole("option")).toHaveCount(8);

  const asking = option(page, "Sketch the Tauri shell");
  await expect(asking).toHaveAttribute("aria-selected", "true");
  await expect(asking).toHaveClass(/mu-item/);
  await expect(asking.locator(".mu-question-ask")).toHaveText("wants to run cargo build");
  await expect(asking).toContainText("⎇ feature/desktop-app in muster");
  await expect(asking.locator(".mu-hints")).toContainText("answer");

  const finish = option(page, "Resize settles once on release");
  await expect(finish).toHaveClass(/mu-row--two/);
  await expect(finish).toContainText("⎇ fix/resize-under-load");
  await expect(finish).toContainText("+64");
  await expect(finish).toContainText("#88");
  await expect(finish.getByRole("img", { name: "not seen" })).toBeVisible();

  const worker = option(page, "Upgrade AGP to 9.1");
  await expect(worker).not.toHaveClass(/mu-row--two/);
  await expect(worker).toContainText("running ./gradlew assembleDebug");

  // The pane beside it: the session, its banner, its terminal and its newest turn.
  const pane = page.getByRole("region", { name: "Sketch the Tauri shell", exact: true });
  await expect(pane.getByRole("heading", { name: "Sketch the Tauri shell" })).toBeVisible();
  await expect(pane).toContainText("182k tokens");
  await expect(pane).toContainText("212 MB, 3% CPU");
  await expect(pane.locator(".mu-banner")).toContainText("Needs you: wants to run cargo build");
  await expect(terminal(page)).toBeVisible();
  const turn = page.getByRole("region", { name: "This turn" });
  await expect(turn).toContainText("Codex, turn 3, changed 3 files");
  await expect(turn.locator(".mu-pill")).toHaveText([
    "link.rs+118−4",
    "main.rs+61",
    "Cargo.toml+24",
  ]);
  await expect(statusBar(page)).toContainText("answer");
  await expect(statusBar(page)).toContainText("put back");
});

test("ARC-24 every reason to ask shows in the item and the banner, and the banner goes as it stops asking", async ({
  page,
}) => {
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
    await expect(option(page, "Sketch the Tauri shell").locator(".mu-question")).toHaveText(text);
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

test("ARC-24 All clear says how many work, with the agents at work below it", async ({ page }) => {
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
  await expect(queue(page).locator(".mu-section")).toHaveText(["Working"]);
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

test("ARC-24 Not seen and Working are the queue filtered, and every queue narrows to a project", async ({
  page,
}) => {
  await page.getByRole("button", { name: /^Not seen/ }).click();
  await expect(page.getByRole("heading", { name: "Not seen", level: 1 })).toBeVisible();
  await expect(queue(page).getByRole("option")).toHaveText([
    /Resize settles/,
    /Explain the release/,
  ]);
  await page.getByRole("button", { name: /^Working/ }).click();
  await expect(page.getByText("Agents at work now, project by project.")).toBeVisible();
  await expect(queue(page).getByRole("option")).toHaveCount(4);
  const worker = option(page, "Upgrade AGP to 9.1");
  await expect(worker).toHaveClass(/mu-row--two/);
  await expect(worker).toContainText("lamphaus");
  await page.getByRole("button", { name: /^lamphaus/ }).click();
  await expect(queue(page).getByRole("option")).toHaveText([/Upgrade AGP/]);
  await page.getByRole("button", { name: /Needs you/ }).click();
  await expect(queue(page).getByRole("option")).toHaveCount(3);
  await page.getByRole("button", { name: /^lamphaus/ }).click();
  await page.getByRole("button", { name: /^kep-site/ }).click();
  await expect(page.getByRole("heading", { name: "All clear" })).toBeVisible();
  await page.getByRole("button", { name: /^Not seen/ }).click();
  await expect(page.getByRole("heading", { name: "All seen" })).toBeVisible();
  await page.getByRole("button", { name: /^Working/ }).click();
  await expect(page.getByRole("heading", { name: "No agent at work" })).toBeVisible();
});

test("ARC-24 ↑↓ move, ⏎ gives the live terminal the keys, ⇧Q gives them back", async ({ page }) => {
  await page.keyboard.press("ArrowDown");
  await expect(option(page, "Share the Gradle cache across worktrees")).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(
    page.getByRole("heading", { name: "Share the Gradle cache across worktrees" }),
  ).toBeVisible();
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
  await expect(option(page, "Explain the release script")).toHaveAttribute("aria-selected", "true");
  await expect(option(page, "Resize settles once on release")).toHaveCount(0);
  // One that leaves gives its place to the next.
  await page.evaluate(() => {
    const id = window.__muster.world.sessions.find(
      (s) => s.title === "Explain the release script",
    )?.id;
    if (id) window.__muster.removeSession(id);
  });
  await expect(option(page, "Split view.rs by responsibility")).toHaveAttribute(
    "aria-selected",
    "true",
  );
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
  await expect(page.getByRole("heading", { name: "All sessions" })).toBeVisible();
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
  await expect(menu.getByRole("menuitem")).toHaveText([
    "RenameR",
    "Pull request in the browserb",
    "Closex",
  ]);
  await page.keyboard.press("Shift+R");
  await expect(menu).toHaveCount(0);
  await expect(name).toBeFocused();
});

test("ARC-24 b says opening the pull request in the browser is coming, and where it is", async ({
  page,
}) => {
  await page.keyboard.press("b");
  await expect(statusBar(page)).toContainText(
    "#91 is at https://github.com/FurkanCodes/muster/pull/91",
  );
  await page.keyboard.press("j");
  await page.keyboard.press("b");
  await expect(statusBar(page)).toContainText("No pull request from this branch is known.");
});

test("PRD-09 x asks first, ⎋ keeps it, and Close session stops it and takes it off the list", async ({
  page,
}) => {
  await page.keyboard.press("x");
  const dialog = page.getByRole("dialog", { name: "Close Sketch the Tauri shell?" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Keep it" })).toBeFocused();
  await page.keyboard.press("j");
  await expect(option(page, "Sketch the Tauri shell")).toHaveAttribute("aria-selected", "true");
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

test("ARC-25 This turn says it is reading, what went wrong, and reads again when the timeline grows", async ({
  page,
}) => {
  await page.evaluate((id) => {
    window.__muster.hold({ Timeline: { workspace: id } });
    window.__muster.updateWorkspace(id, { timeline: 5 });
  }, desktop);
  const turn = page.getByRole("region", { name: "This turn" });
  await expect(turn).toContainText("Reading the timeline…");
  await page.evaluate((id) => {
    window.__muster.failNext(
      { Timeline: { workspace: id } },
      "the timeline is busy: try again in a moment",
    );
    window.__muster.release({ Timeline: { workspace: id } });
  }, desktop);
  await expect(turn.getByRole("alert")).toContainText("the timeline is busy");
  await page.evaluate((id) => {
    const newest = window.__muster.world.timelines?.[id]?.entries[0];
    if (newest) newest.summary = null;
  }, desktop);
  await turn.getByRole("button", { name: "Try again" }).click();
  await expect(turn).toContainText("Codex, turn 3, changed no files");
  await expect(turn.locator(".mu-pill")).toHaveCount(0);
  await page.evaluate((id) => {
    const mock = window.__muster;
    if (mock.world.timelines) mock.world.timelines[id] = { entries: [] };
    mock.updateWorkspace(id, { timeline: 6 });
  }, desktop);
  await expect(turn).toContainText("None kept yet: each agent's turn here is, from now on.");
});

test("PRD-14 Put back and See the diff open the changes on the turn; Put back waits while the agent works", async ({
  page,
}) => {
  const turn = page.getByRole("region", { name: "This turn" });
  const changes = page.getByRole("button", { name: "Changes", exact: true });
  await turn.getByRole("button", { name: "Put back" }).click();
  await expect(changes).toHaveAttribute("aria-pressed", "true");
  await changes.click();
  await turn.getByRole("button", { name: "See the diff" }).click();
  await expect(changes).toHaveAttribute("aria-pressed", "true");
  await changes.click();
  await turn.getByRole("button", { name: /link\.rs, its diff/ }).click();
  await expect(changes).toHaveAttribute("aria-pressed", "true");
  await changes.click();
  await page.evaluate((id) => window.__muster.setStatus(id, { state: "running" }), codex);
  const putBack = turn.getByRole("button", { name: "Put back" });
  await expect(putBack).toBeDisabled();
  await expect(putBack).toHaveAttribute("title", "Wait for its turn to end");
  await page.keyboard.press("z");
  await expect(statusBar(page)).toContainText("Wait for its turn to end.");
  await page.keyboard.press("d");
  await expect(changes).toHaveAttribute("aria-pressed", "true");
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

test("ARC-24 narrowed to its least, the sidebar folds and the session keeps 520 wide", async ({
  page,
}) => {
  await page.setViewportSize({ width: 960, height: 600 });
  await expect(page.getByText("Projects", { exact: true })).toHaveCount(0);
  const pane = page.getByRole("region", { name: "Sketch the Tauri shell", exact: true });
  expect((await pane.boundingBox())?.width).toBeGreaterThanOrEqual(520);
  await expect(page.getByRole("button", { name: "Changes", exact: true })).toBeInViewport();
  // This turn's head stays one line: its description is cut before its buttons move.
  const turn = page.getByRole("region", { name: "This turn" });
  const putBack = await turn.getByRole("button", { name: "Put back" }).boundingBox();
  const diff = await turn.getByRole("button", { name: "See the diff" }).boundingBox();
  expect(diff?.y).toBe(putBack?.y);
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(page.getByText("Projects", { exact: true })).toBeVisible();
});

test("ARC-24 the asking session's gemini item shows its own question", async ({ page }) => {
  await expect(
    option(page, "Share the Gradle cache across worktrees").locator(".mu-question"),
  ).toHaveText("asks which cache folder to use");
  await page.evaluate((id) => window.__muster.setStatus(id, { state: "running" }), gemini);
  await expect(option(page, "Share the Gradle cache across worktrees")).not.toHaveClass(/mu-item/);
});
