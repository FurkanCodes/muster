import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

declare global {
  interface Window {
    __muster: MockBridge;
  }
}
const id = (n: number) => `01JD${String(n).padStart(22, "0")}`;
const ticket = (page: Page, key: string) =>
  page.getByRole("option", { name: new RegExp(`^${key} `) });
/** The window over the Tickets board's day: its agents on their tickets. */
async function start(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.evaluate(() => window.__muster.agentsOnTickets());
}
async function tickets(page: Page) {
  await page.keyboard.press("Meta+3");
  await expect(page.getByRole("heading", { name: "Tickets", exact: true })).toBeVisible();
}

test("ARC-24 the Tickets board: groups, agents, pull request, author, labels and text", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await expect(ticket(page, "#214")).toHaveAttribute("aria-selected", "true");
  await expect(ticket(page, "#214").getByRole("img", { name: "needs you" })).toBeVisible();
  await expect(ticket(page, "#214")).toContainText("#91");
  await expect(page.getByText("182k tokens", { exact: true })).toHaveCount(2);
  await expect(page.getByText("opened by FurkanCodes")).toBeVisible();
  await expect(page.locator(".mu-label", { hasText: "desktop" })).toBeVisible();
  await expect(page.getByRole("article")).toContainText("Done when");
  await expect(ticket(page, "KEP-31")).toHaveCount(1);
  await expect(
    page.locator(".mu-group-name").filter({ hasText: "kep-site, kep-api" }),
  ).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await expect(ticket(page, "#205")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Needs you", exact: true })).toBeVisible();
  await page.keyboard.press("Meta+3");
  await expect(ticket(page, "#214")).toHaveAttribute("aria-selected", "true");
});

test("ARC-25 loading lists and text, tracker failure and r to fetch again", async ({ page }) => {
  await start(page);
  await page.evaluate(() => {
    window.__muster.hold("ListIssues");
    window.__muster.failNext("ListIssues", "gh is not signed in");
  });
  await tickets(page);
  await expect(page.getByRole("status", { name: "Loading tickets" })).toHaveCount(4);
  await page.evaluate(() => window.__muster.release("ListIssues"));
  await expect(page.getByRole("alert")).toContainText(
    "Its tickets could not be fetched: gh is not signed in",
  );
  await page.keyboard.press("r");
  await expect(ticket(page, "#214")).toBeVisible();
  await page.evaluate(() => window.__muster.hold("ViewIssue"));
  await ticket(page, "#214").click();
  await expect(page.getByRole("status", { name: "Loading ticket", exact: true })).toBeVisible();
  await page.evaluate(() => {
    window.__muster.failNext("ViewIssue", "tracker cannot be reached");
    window.__muster.release("ViewIssue");
  });
  await expect(page.getByRole("alert")).toContainText(
    "Its text could not be fetched: tracker cannot be reached",
  );
  await page.keyboard.press("r");
  await expect(page.getByRole("article")).toContainText("Done when");
});

test("ARC-24 empty project, only-mine empty and no projects", async ({ page }) => {
  await start(page);
  await page.evaluate(() => {
    for (const list of Object.values(window.__muster.world.tickets)) list.issues = [];
  });
  await tickets(page);
  await expect(page.getByText("no open tickets", { exact: true })).toHaveCount(2);
  await expect(page.getByText("none of yours open", { exact: true })).toBeVisible();
  await expect(page.getByText("Select a ticket to see it here.")).toBeVisible();
  await page.evaluate(() => {
    const mock = window.__muster;
    const gone = mock.world.projects.map((p) => p.id);
    mock.world.projects = [];
    mock.publish({ gone: { projects: gone, sessions: [], workspaces: [] } });
  });
  await expect(
    page.getByText("No projects yet: add one, and its open tickets show here."),
  ).toBeVisible();
});

test("PRD-01 Mine and All change the project's setting and surface refusals", async ({ page }) => {
  await start(page);
  await tickets(page);
  const mine = page.getByRole("button", { name: "Mine", exact: true }).first();
  await page.evaluate(() =>
    window.__muster.failNext("ChangeSetting", "settings file cannot be written"),
  );
  await mine.click();
  await expect(page.getByText("settings file cannot be written")).toBeVisible();
  await expect(mine).toHaveAttribute("aria-pressed", "false");
  await mine.click();
  await expect(mine).toHaveAttribute("aria-pressed", "true");
  expect(
    await page.evaluate(() =>
      window.__muster.sent.some(
        (c) =>
          typeof c !== "string" &&
          "ChangeSetting" in c &&
          c.ChangeSetting.change &&
          "OnlyMyTickets" in c.ChangeSetting.change &&
          c.ChangeSetting.change.OnlyMyTickets,
      ),
    ),
  ).toBe(true);
  await page.getByRole("button", { name: "All", exact: true }).first().click();
  await expect(mine).toHaveAttribute("aria-pressed", "false");
  // Projects sharing a tracker change together, so they go on sharing it.
  const kep = page.getByRole("group", { name: "Which of kep-site, kep-api's tickets" });
  await kep.getByRole("button", { name: "Mine" }).click();
  await expect(kep.getByRole("button", { name: "Mine" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByText("Linear (KEP), only yours")).toBeVisible();
  await expect(ticket(page, "KEP-31")).toHaveCount(1);
});

test("ARC-24 Mine and All keep the list shown while it is fetched again, its selected ticket too", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await ticket(page, "#205").click();
  await expect(ticket(page, "#205")).toHaveAttribute("aria-selected", "true");
  await page.evaluate(() => window.__muster.hold("ListIssues"));
  await page.getByRole("button", { name: "Mine", exact: true }).first().click();
  // Asked again, a Loading rule under the project's heading says so; the list as it was stays under it.
  const muster = page.getByRole("region", { name: "muster's tickets" });
  await expect(muster.getByRole("status", { name: "Loading tickets" })).toBeVisible();
  await expect(page.getByRole("status", { name: "Loading tickets" })).toHaveCount(1);
  await expect(ticket(page, "#205")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("heading", { name: "Resize settles once on release" })).toBeVisible();
  await page.evaluate(() => window.__muster.release("ListIssues"));
  await expect(page.getByRole("status", { name: "Loading tickets" })).toHaveCount(0);
});

test("PRD-07 PRD-10 ticket HTML, images and links never load or execute", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("untrusted.invalid")) requests.push(request.url());
  });
  await start(page);
  await page.evaluate((project) => {
    const detail = window.__muster.world.ticketTexts[`${project}:214`];
    if (detail)
      detail.body =
        '<img src="https://untrusted.invalid/image"><script>window.ticketExecuted=true</script>\n\n![pixel](https://untrusted.invalid/pixel)\n\n[Read more](https://untrusted.invalid/read)';
  }, id(1));
  await tickets(page);
  const body = page.getByRole("article");
  await expect(body).toContainText('<img src="https://untrusted.invalid/image">');
  await expect(body).toContainText("![pixel](https://untrusted.invalid/pixel)");
  await expect(body).toContainText("Read more (https://untrusted.invalid/read)");
  await expect(body.locator("img, script, a, iframe")).toHaveCount(0);
  expect(requests).toEqual([]);
  expect(await page.evaluate(() => "ticketExecuted" in window)).toBe(false);
});

test("PRD-01 Assign board: Tab chooses agent, arrows choose base, Enter starts once", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await ticket(page, "#217").click();
  await expect(page.getByText("No agent on it yet")).toBeVisible();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Assign an agent to #217" });
  await expect(dialog.getByRole("radio", { name: /Claude Code/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(dialog.getByRole("radio", { name: /Shell/ })).toHaveCount(0);
  await expect(dialog).toContainText("marked as a description of the work, never as instructions");
  await page.keyboard.press("Tab");
  await expect(dialog.getByRole("radio", { name: /Codex/ })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("ArrowRight");
  await expect(dialog.getByRole("button", { name: "Made on top of", exact: true })).toContainText(
    "muster/issue-214-desktop-app",
  );
  await page.evaluate(() => window.__muster.hold("StartFromIssue"));
  await page.keyboard.press("Enter");
  await expect(dialog.getByRole("button", { name: "Starting…" })).toBeDisabled();
  await page.keyboard.press("Enter");
  expect(
    await page.evaluate(
      () =>
        window.__muster.sent.filter((c) => typeof c !== "string" && "StartFromIssue" in c).length,
    ),
  ).toBe(1);
  await page.evaluate(() => window.__muster.release("StartFromIssue"));
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Sessions", exact: true })).toBeVisible();
  const command = await page.evaluate(() =>
    window.__muster.sent.find((c) => typeof c !== "string" && "StartFromIssue" in c),
  );
  expect(command).toEqual({
    StartFromIssue: {
      project: id(1),
      key: "217",
      harness: "codex",
      size: { rows: 24, cols: 80 },
      another: false,
      on: id(11),
    },
  });
  await tickets(page);
  await page.keyboard.press("a");
  await expect(page.getByRole("dialog").getByRole("radio").first()).toHaveAccessibleName(
    /Codex.*last chosen/,
  );
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("ARC-24 shared tickets ask which project and remember it", async ({ page }) => {
  await start(page);
  await tickets(page);
  await ticket(page, "KEP-28").click();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "Project", exact: true })).toContainText(
    "kep-site",
  );
  await page.keyboard.press("ArrowDown");
  await expect(dialog.getByRole("button", { name: "Project", exact: true })).toContainText(
    "kep-api",
  );
  await page.keyboard.press("Enter");
  // Assigned once the window shows the agent: until the daemon answers, the
  // dialog, busy, still holds the keys, ⌘3 among them.
  await expect(
    page.getByRole("heading", { name: "Sessions", level: 1, exact: true }),
  ).toBeVisible();
  await expect(dialog).toHaveCount(0);
  await tickets(page);
  await page.keyboard.press("a");
  await expect(dialog.getByRole("button", { name: "Project", exact: true })).toContainText(
    "kep-api",
  );
  await page.keyboard.press("Escape");
});

test("PRD-01 a refused start stays in the dialog and can be tried again", async ({ page }) => {
  await start(page);
  await tickets(page);
  await ticket(page, "#217").click();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.evaluate(() => window.__muster.failNext("StartFromIssue", "Codex is not installed"));
  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText("Codex is not installed");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Sessions", exact: true })).toBeVisible();
});

test("PRD-01 close asks first; Escape keeps it, refusal keeps it, success offers cleanup", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await page.keyboard.press("x");
  let dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("button", { name: "Keep it open" })).toBeFocused();
  await expect(dialog).toContainText(
    "Everyone watching FurkanCodes/muster #214 is told. Muster changes nothing else in the tracker.",
  );
  await page.keyboard.press("Escape");
  expect(
    await page.evaluate(() =>
      window.__muster.sent.some((c) => typeof c !== "string" && "CloseIssue" in c),
    ),
  ).toBe(false);
  // ⏎ on the answer that has the focus keeps it: closing tells everyone watching.
  await page.keyboard.press("x");
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  await page.keyboard.press("x");
  await page.evaluate(() =>
    window.__muster.failNext("CloseIssue", "not allowed to close this ticket", "bad_request"),
  );
  await page.keyboard.press("ArrowRight");
  await expect(dialog.getByRole("button", { name: "Close ticket" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(dialog.getByRole("alert")).toContainText("not allowed to close this ticket");
  await expect(ticket(page, "#214")).toHaveCount(1);
  await dialog.getByRole("button", { name: "Close ticket" }).click();
  dialog = page.getByRole("dialog", { name: "Closed #214. Clean up after it?" });
  await expect(dialog).toContainText("1 agent still running: Sketch the Tauri shell");
  await expect(dialog).toContainText(
    `⎇ muster/issue-214-desktop-app-sessions-list-and-one-live, once nothing runs in it`,
  );
  await expect(dialog).toContainText("its branch");
  await dialog.getByRole("button", { name: "Leave them" }).click();
  await expect(ticket(page, "#214")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.__muster.world.workspaces.some((w) => w.id === "01JD0000000000000000000011"),
    ),
  ).toBe(true);
});

test("PRD-14 cleanup waits for agents, keeps branches, and separately asks to discard dirty files", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await page.keyboard.press("x");
  await page.getByRole("dialog").getByRole("button", { name: "Close ticket" }).click();
  await expect(page.getByRole("dialog", { name: "Closed #214. Clean up after it?" })).toBeVisible();
  await page.evaluate(() => window.__muster.hold("StopSession"));
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Cleaning up after #214…" })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.__muster.sent.some((c) => typeof c !== "string" && "RemoveWorktree" in c),
    ),
  ).toBe(false);
  await page.evaluate(() => window.__muster.release("StopSession"));
  const dirty = page.getByRole("dialog", {
    name: "Remove ⎇ muster/issue-214-desktop-app-sessions-list-and-one-live?",
  });
  await expect(dirty).toContainText(
    "Removing deletes its folder and those changes. The branch stays.",
  );
  await expect(dirty.getByRole("button", { name: "Keep it" })).toBeFocused();
  await expect(dirty.getByRole("button", { name: "Remove with its changes" })).toHaveAttribute(
    "title",
    "Remove with its changes (⌘⏎)",
  );
  await page.keyboard.press("Meta+Enter");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.__muster.world.workspaces.some((w) => w.id === "01JD0000000000000000000011"),
    ),
  ).toBe(false);
  expect(
    await page.evaluate(() =>
      window.__muster.sent.filter((c) => typeof c !== "string" && "RemoveWorktree" in c),
    ),
  ).toEqual([
    { RemoveWorktree: { workspace: id(11), force: false } },
    { RemoveWorktree: { workspace: id(11), force: true } },
  ]);
});

test("ARC-24 Select supports keyboard listbox choice and cancellation", async ({ page }) => {
  await start(page);
  await tickets(page);
  await ticket(page, "#217").click();
  await page.keyboard.press("Enter");
  const select = page
    .getByRole("dialog")
    .getByRole("button", { name: "Made on top of", exact: true });
  await select.click();
  await expect(page.getByRole("listbox", { name: "Made on top of" })).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(select).toContainText("muster/issue-214-desktop-app");
  await select.click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox", { name: "Made on top of" })).toHaveCount(0);
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Escape");
});

test("PRD-07 ARC-24 browser and close use saved key bindings", async ({ page }) => {
  await start(page);
  await page.evaluate(() => {
    const mock = window.__muster;
    mock.world.preferences.keys.ticket_in_browser = ["shift-b"];
    mock.world.preferences.keys.close_ticket = ["alt-x"];
    mock.publish({ preferences: { ...mock.world.preferences } });
  });
  await tickets(page);
  expect(await page.evaluate(() => window.__muster.openedUrls)).toEqual([]);
  await page.keyboard.press("b");
  expect(await page.evaluate(() => window.__muster.openedUrls)).toEqual([]);
  await page.keyboard.press("Shift+B");
  await expect
    .poll(() => page.evaluate(() => window.__muster.openedUrls))
    .toEqual(["https://github.com/FurkanCodes/muster/issues/214"]);
  await page.getByRole("button", { name: "Open in the browser (⇧B)" }).click();
  await expect.poll(() => page.evaluate(() => window.__muster.openedUrls.length)).toBe(2);
  await page.keyboard.press("x");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.keyboard.press("Alt+x");
  await expect(page.getByRole("dialog", { name: "Close #214 on GitHub?" })).toBeVisible();
  await page.keyboard.press("Escape");
});

test("PRD-01 a ticket without worktrees closes without a cleanup offer", async ({ page }) => {
  await start(page);
  await tickets(page);
  await ticket(page, "#217").click();
  await page.keyboard.press("x");
  await page.getByRole("dialog").getByRole("button", { name: "Close ticket" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(ticket(page, "#217")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.__muster.sent.some((c) => typeof c !== "string" && "RemoveWorktree" in c),
    ),
  ).toBe(false);
});

test("PRD-14 keeping dirty files never sends force removal", async ({ page }) => {
  await start(page);
  await tickets(page);
  await page.keyboard.press("x");
  await page.getByRole("dialog").getByRole("button", { name: "Close ticket" }).click();
  await expect(page.getByRole("dialog", { name: "Closed #214. Clean up after it?" })).toBeVisible();
  await page.keyboard.press("Enter");
  const dirty = page.getByRole("dialog", {
    name: "Remove ⎇ muster/issue-214-desktop-app-sessions-list-and-one-live?",
  });
  await expect(dirty).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  expect(
    await page.evaluate(() =>
      window.__muster.world.workspaces.some((w) => w.id === "01JD0000000000000000000011"),
    ),
  ).toBe(true);
  expect(
    await page.evaluate(() =>
      window.__muster.sent.some(
        (c) => typeof c !== "string" && "RemoveWorktree" in c && c.RemoveWorktree.force,
      ),
    ),
  ).toBe(false);
});

test("PRD-01 another agent starts in its own worktree on the chosen branch", async ({ page }) => {
  await start(page);
  await tickets(page);
  await page.keyboard.press("a");
  await expect(page.getByRole("dialog", { name: "Assign an agent to #214" })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Sessions", exact: true })).toBeVisible();
  expect(
    await page.evaluate(() =>
      window.__muster.sent.find((c) => typeof c !== "string" && "StartFromIssue" in c),
    ),
  ).toEqual({
    StartFromIssue: {
      project: id(1),
      key: "214",
      harness: "claude",
      size: { rows: 24, cols: 80 },
      another: true,
      on: id(11),
    },
  });
  expect(
    await page.evaluate(() => window.__muster.world.workspaces.at(-1)?.stacked_on?.branch),
  ).toBe("muster/issue-214-desktop-app-sessions-list-and-one-live");
  await tickets(page);
  await expect(ticket(page, "#214").locator(".mu-ticket-agent")).toHaveCount(2);
});

test("ARC-24 a ticket with no description says so", async ({ page }) => {
  await start(page);
  await page.evaluate((project) => {
    const detail = window.__muster.world.ticketTexts[`${project}:214`];
    if (detail) detail.body = "";
  }, id(1));
  await tickets(page);
  await expect(page.getByRole("article")).toHaveText("(No description.)");
});

test("ARC-24 a ticket row flashes once when an agent on it comes to need the person", async ({
  page,
}) => {
  await start(page);
  await tickets(page);
  await expect(ticket(page, "#214")).not.toHaveClass(/mu-flash/);
  await expect(ticket(page, "#205")).not.toHaveClass(/mu-flash/);
  await page.evaluate(() => {
    const mock = window.__muster;
    const tree = mock.world.workspaces.find((w) => w.kind.branch.startsWith("muster/issue-205-"));
    const agent = mock.world.sessions.find(
      (s) => s.workspace === tree?.id && s.status.state !== "needs_you",
    );
    if (agent) mock.setStatus(agent.id, { state: "needs_you", reason: "permission" });
  });
  await expect(ticket(page, "#205")).toHaveClass(/mu-flash--waiting/);
  await expect(ticket(page, "#214")).not.toHaveClass(/mu-flash/);
});
