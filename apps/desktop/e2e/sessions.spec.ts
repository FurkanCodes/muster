import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// All sessions, its panels and full screen, end to end over the mock daemon
// (the AllSessions, Working, FullScreen and PluginPanel boards).

declare global {
  interface Window {
    __muster: MockBridge;
  }
}

const id = (n: number) => `01JD${String(n).padStart(22, "0")}`;
const line = (page: Page, name: string | RegExp) =>
  page.getByRole("treeitem", { name, exact: typeof name === "string" });
const region = (page: Page, name: string) => page.getByRole("region", { name, exact: true });

async function allSessions(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Needs you" })).toBeVisible();
  await page.keyboard.press("Meta+2");
  await expect(page.getByRole("heading", { name: "All sessions" })).toBeVisible();
}

/** What a session's terminal shows (xterm draws on the GPU). */
const shown = (page: Page, session: string) =>
  page.evaluate(
    (session) =>
      (
        window as unknown as { __musterTerminals?: Record<string, () => string> }
      ).__musterTerminals?.[session]?.() ?? "",
    session,
  );

test("ARC-24 the tree: projects, worktrees, sessions, shells beside their chat", async ({
  page,
}) => {
  await allSessions(page);
  const tree = page.getByRole("tree", { name: "Projects, worktrees and sessions" });
  await expect(line(page, "muster")).toHaveAttribute("aria-expanded", "true");
  await expect(line(page, "muster")).toContainText("~/muster");
  await expect(line(page, "fix/resize-under-load")).toContainText("+64−22");
  await expect(line(page, "fix/resize-under-load")).toContainText("overlap");
  await expect(line(page, "fix/resize-under-load")).toContainText("#88approved");
  await expect(line(page, /^Sketch the Tauri shell/)).toContainText("wants to run cargo build");
  await expect(line(page, "Shell: git log")).toContainText("└");
  await expect(tree.getByRole("img", { name: "not seen" })).toHaveCount(2);
  // The first session is selected, its pane beside the tree, live.
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("heading", { name: "Split view.rs by responsibility" }),
  ).toBeVisible();
  await expect.poll(() => shown(page, id(100))).toContain("Splitting sessions.rs");
  await expect(page.locator(".mu-statusbar")).toContainText("type in it");
});

test("ARC-24 ↑↓ and j k move, ⏎ types in the session, ⇧Q gives the keys back", async ({ page }) => {
  await allSessions(page);
  await page.keyboard.press("ArrowDown");
  await expect(line(page, "fix/resize-under-load")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("region", { name: "Selected line" })).toContainText(
    "⎇ fix/resize-under-load",
  );
  await page.keyboard.press("j");
  await expect(line(page, /^Flaky resize/)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("k");
  await page.keyboard.press("k");
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect(page.locator(".mu-statusbar")).toContainText(
    "Typing in Split view.rs by responsibility. Every key goes to Claude Code.",
  );
  await page.keyboard.type("hi");
  await expect.poll(() => shown(page, id(100))).toContain("hi");
  await page.keyboard.press("Shift+Q");
  await page.keyboard.press("ArrowDown");
  await expect(line(page, "fix/resize-under-load")).toHaveAttribute("aria-selected", "true");
});

test("ARC-24 folds: ← → and e, a click on the triangle, the folded mark, kept across restarts", async ({
  page,
}) => {
  await allSessions(page);
  // On a session, ← goes out to its worktree, then folds it.
  await page.keyboard.press("ArrowLeft");
  await expect(line(page, "main").first()).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowLeft");
  const resize = line(page, "fix/resize-under-load");
  await expect(resize).toHaveAttribute("aria-expanded", "false");
  // Folded, it says how the most urgent of what it hides stands: the news first.
  await expect(resize).toContainText("2 folded");
  await expect(resize.getByRole("img", { name: "finished" })).toBeVisible();
  await expect(resize.getByRole("img", { name: "not seen" })).toBeVisible();
  await expect(line(page, /^Flaky resize/)).toHaveCount(0);
  await page.keyboard.press("ArrowRight");
  await expect(resize).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowRight");
  await expect(line(page, /^Flaky resize/)).toHaveAttribute("aria-selected", "true");
  // e on a session folds its worktree, which is then selected.
  await page.keyboard.press("e");
  await expect(resize).toHaveAttribute("aria-selected", "true");
  await expect(resize).toHaveAttribute("aria-expanded", "false");
  // A project folds by a click on its triangle: asking first.
  await line(page, "lamphaus").locator(".mu-tree-fold").click();
  await expect(line(page, "lamphaus")).toContainText("3 folded");
  await expect(line(page, "lamphaus").getByRole("img", { name: "needs you" })).toBeVisible();
  // A line with nothing under it does not fold, and says so.
  // kep-site's checkout, where no session works.
  await page.locator(`#tree-${id(30)}`).click();
  await page.keyboard.press("e");
  await expect(page.locator(".mu-note")).toHaveText("No sessions here to fold away.");
  // The folds are the window's own, kept across a restart.
  await page.reload();
  await page.keyboard.press("Meta+2");
  await expect(line(page, "fix/resize-under-load")).toHaveAttribute("aria-expanded", "false");
  await expect(line(page, "lamphaus")).toHaveAttribute("aria-expanded", "false");
});

test("ARC-24 narrowing to a project shows its lines only", async ({ page }) => {
  await allSessions(page);
  await page
    .getByRole("navigation", { name: "Screens" })
    .getByRole("button", { name: /^lamphaus/ })
    .click();
  await expect(line(page, "lamphaus")).toBeVisible();
  await expect(line(page, "muster")).toHaveCount(0);
  await expect(line(page, /^Share the Gradle cache/)).toBeVisible();
});

test("ARC-24 out of reach, and no projects yet", async ({ page }) => {
  await allSessions(page);
  await page.evaluate(() =>
    window.__muster.setLink({ state: "down", reason: "the daemon is not running", since: 0 }),
  );
  await expect(page.getByRole("alert")).toContainText("Out of reach: the daemon is not running");
  await expect(line(page, "muster")).toBeVisible();
  await page.evaluate(() => {
    const mock = window.__muster;
    mock.world.projects = [];
    mock.world.workspaces = [];
    mock.world.sessions = [];
    mock.publish({ reset: true });
  });
  await expect(page.getByRole("heading", { name: "No projects yet" })).toBeVisible();
});

test("PRD-15 T opens the shell beside the agent, live and typed in; T puts it away", async ({
  page,
}) => {
  await allSessions(page);
  await line(page, /^Sketch the Tauri shell/).click();
  await page.keyboard.press("T");
  const shell = region(page, "Shell");
  await expect(shell).toContainText("git log");
  await expect(shell).toHaveClass(/has-keys/);
  await expect(page.getByRole("button", { name: "Shell", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.keyboard.type("ls");
  await expect.poll(() => shown(page, id(102))).toContain("ls");
  await page.keyboard.press("Shift+Q");
  await page.keyboard.press("T");
  await expect(shell).toHaveCount(0);
  await expect(page.locator(".mu-note")).toHaveText("The shell is put away; it runs on.");
  // In a workspace with none, one is started for it, and shows in the tree.
  await line(page, /^Flaky resize/).click();
  await page.keyboard.press("T");
  await expect(region(page, "Shell")).toContainText("zsh");
  await expect(line(page, "Shell: zsh")).toContainText("at its prompt");
  const started = await page.evaluate(() =>
    window.__muster.sent.some(
      (command) => typeof command === "object" && "StartSession" in command,
    ),
  );
  expect(started).toBe(true);
  // From a worktree's line, the shell opens there and is selected.
  await page.keyboard.press("Shift+Q");
  await line(page, "feature/desktop-app").click();
  await page.keyboard.press("T");
  await expect(line(page, "Shell: git log")).toHaveAttribute("aria-selected", "true");
});

test("PRD-15 a plugin's panel opens beside the session on its key, and its x puts it away", async ({
  page,
}) => {
  await allSessions(page);
  await page.keyboard.press("G");
  const graph = region(page, "Session graph");
  await expect(graph).toContainText("graph, from the plugin");
  await expect(line(page, "Panel: Session graph")).toContainText("G");
  // Another session of another worktree shows none.
  await line(page, /^Flaky resize/).click();
  await expect(graph).toHaveCount(0);
  await line(page, /^Split view\.rs/).click();
  await graph.getByRole("button", { name: "Put the graph away" }).click();
  await expect(graph).toHaveCount(0);
  await expect(line(page, "Panel: Session graph")).toHaveCount(0);
  // A worktree's line has no session to open it beside.
  await line(page, "main").first().click();
  await page.keyboard.press("G");
  await expect(page.locator(".mu-note")).toHaveText("Select a session to open graph beside it.");
});

test("ARC-24 panels move by their heads, resize by the line between, and are kept", async ({
  page,
}) => {
  await allSessions(page);
  await page.keyboard.press("T");
  await page.keyboard.press("Shift+Q");
  const agent = region(page, "Claude Code");
  const shell = region(page, "Shell");
  await expect(shell).toBeVisible();
  const separator = page.getByRole("separator");
  await expect(separator).toHaveAttribute("aria-orientation", "vertical");

  // The shell's head dropped on the lower part of the agent's panel: under it.
  const head = shell.locator(".mu-panel-head");
  const from = await head.boundingBox();
  const to = await agent.boundingBox();
  if (!from || !to) throw new Error("not laid out");
  await page.mouse.move(from.x + 60, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + to.width / 2, to.y + to.height * 0.85, { steps: 8 });
  await expect(page.locator(".panels-landing")).toBeVisible();
  await page.mouse.up();
  await expect(separator).toHaveAttribute("aria-orientation", "horizontal");
  await expect
    .poll(() => page.evaluate(() => window.__muster.world.preferences.panels))
    .toBe("list | (session / shell) 60% | (changes / plugin)");

  // The line between them, dragged, shares their place.
  const bar = await separator.boundingBox();
  if (!bar) throw new Error("no line");
  await page.mouse.move(bar.x + bar.width / 2, bar.y + bar.height / 2);
  await page.mouse.down();
  await page.mouse.move(bar.x + bar.width / 2, bar.y - 150, { steps: 6 });
  await page.mouse.up();
  await expect
    .poll(() => page.evaluate(() => window.__muster.world.preferences.panels))
    .toMatch(/^list \| \(session \d+% \/ shell\) 60% \| \(changes \/ plugin\)$/);
  const share = Number(
    (await page.evaluate(() => window.__muster.world.preferences.panels)).match(
      /session (\d+)%/,
    )?.[1],
  );
  expect(share).toBeLessThan(50);

  // Pressed and not moved, a head gives its panel the keys.
  await head.click();
  await expect(shell).toHaveClass(/has-keys/);
  await page.keyboard.press("Shift+Q");

  // Esc lets go of a drag, the layout as it was.
  const before = await page.evaluate(() => window.__muster.world.preferences.panels);
  const again = await shell.locator(".mu-panel-head").boundingBox();
  if (!again) throw new Error("no head");
  await page.mouse.move(again.x + 60, again.y + again.height / 2);
  await page.mouse.down();
  await page.mouse.move(again.x + 5, again.y + 200, { steps: 6 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator(".panels-landing")).toHaveCount(0);
  expect(await page.evaluate(() => window.__muster.world.preferences.panels)).toBe(before);
});

test("ARC-24 the layout is read from the person's settings", async ({ page }) => {
  await allSessions(page);
  await page.evaluate(() => {
    const mock = window.__muster;
    mock.world.preferences = {
      ...mock.world.preferences,
      panels: "list | (shell / session) | changes | plugin",
    };
    mock.publish({ preferences: mock.world.preferences });
  });
  await page.keyboard.press("T");
  const shell = await region(page, "Shell").boundingBox();
  const agent = await region(page, "Claude Code").boundingBox();
  expect(shell && agent && shell.y < agent.y).toBe(true);
});

test("ARC-24 full screen on f: the session and its panels fill the window; Esc gives it back", async ({
  page,
}) => {
  await allSessions(page);
  await page.keyboard.press("T");
  await page.keyboard.press("Shift+Q");
  await page.keyboard.press("f");
  const sidebar = page.getByRole("navigation", { name: "Screens" });
  await expect(sidebar).toBeHidden();
  await expect(page.getByRole("tree")).toBeHidden();
  const title = page.locator(".mu-titlebar");
  await expect(title).toContainText("Split view.rs by responsibility");
  await expect(title).toContainText("⎇ main");
  await expect(region(page, "Claude Code")).toHaveClass(/has-keys/);
  await expect(region(page, "Shell")).toBeVisible();
  await page.keyboard.type("x");
  await expect.poll(() => shown(page, id(100))).toContain("x");
  // ⇧Q gives the keys back; the window stays the session's until f or Esc.
  await page.keyboard.press("Shift+Q");
  await expect(page.locator(".mu-statusbar")).toContainText("leave full screen");
  await expect(sidebar).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(sidebar).toBeVisible();
  await expect(page.getByRole("tree")).toBeVisible();
  // f again, and the button in the title bar gives the window back.
  await page.keyboard.press("f");
  await page.keyboard.press("Shift+Q");
  await page.getByRole("button", { name: "Leave full screen" }).click();
  await expect(sidebar).toBeVisible();
  // On a worktree's line there is no session to fill the window with.
  await line(page, "main").first().click();
  await page.keyboard.press("f");
  await expect(page.locator(".mu-note")).toHaveText("Select a session to open it full screen.");
});

test("ARC-24 m moves the panels with the keys: ⇥ another, the arrows past a neighbour, ⏎ keeps", async ({
  page,
}) => {
  await allSessions(page);
  await page.keyboard.press("T");
  await page.keyboard.press("Shift+Q");
  await page.keyboard.press("G");
  await expect(region(page, "Session graph")).toBeVisible();
  await page.keyboard.press("m");
  await expect(page.locator(".mu-note")).toContainText("Moving the agent");
  // The agent to the right, past the shell and the graph beside it.
  await page.keyboard.press("ArrowRight");
  const agent = await region(page, "Claude Code").boundingBox();
  const shell = await region(page, "Shell").boundingBox();
  expect(agent && shell && agent.x > shell.x).toBe(true);
  // The arrows were the panels', not the tree's.
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect
    .poll(() => page.evaluate(() => window.__muster.world.preferences.panels))
    .toContain("session");
  const kept = await page.evaluate(() => window.__muster.world.preferences.panels);
  expect(kept).not.toBe("list | session 60% | (changes / shell / plugin)");
});
