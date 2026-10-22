import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// The All list in the sidebar, the session beside it, its panels and full
// screen, end to end over the mock daemon.

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
  await expect(page.getByRole("listbox", { name: "Needs you sessions" })).toBeVisible();
  await page.keyboard.press("Meta+1");
  await expect(page.getByRole("tree", { name: "Projects and sessions" })).toBeVisible();
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

test("ARC-24 the All list: projects and their sessions, each worktree said over its session", async ({
  page,
}) => {
  await allSessions(page);
  const tree = page.getByRole("tree", { name: "Projects and sessions" });
  await expect(line(page, "muster")).toHaveAttribute("aria-expanded", "true");
  await expect(line(page, /^Sketch the Tauri shell/)).toContainText("wants to run cargo build");
  // No worktree's line, and no shell beside a chat: the chat's head and its shell icon say them.
  await expect(line(page, "fix/resize-under-load")).toHaveCount(0);
  await expect(line(page, "Shell: git log")).toHaveCount(0);
  await expect(tree.getByRole("img", { name: "not seen" })).toHaveCount(2);
  // The first session is selected, its pane beside the list, live.
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("heading", { name: "Split view.rs by responsibility" }),
  ).toBeVisible();
  await expect.poll(() => shown(page, id(100))).toContain("Splitting sessions.rs");
  await expect(page.locator(".mu-statusbar")).toContainText("type in it");
  await line(page, /^Flaky resize/).click();
  const head = region(page, "Flaky resize test under load");
  await expect(head).toContainText("⎇ fix/resize-under-load");
  await expect(head).toContainText("+64−22");
  await expect(head).toContainText("overlap");
  await expect(head).toContainText("#88approved");
});

test("ARC-24 ↑↓ and j k move, ⏎ types in the session, ⇧Q gives the keys back", async ({ page }) => {
  await allSessions(page);
  await page.keyboard.press("ArrowDown");
  await expect(line(page, /^Flaky resize/)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("j");
  await expect(line(page, /^Resize settles/)).toHaveAttribute("aria-selected", "true");
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
  await expect(line(page, /^Flaky resize/)).toHaveAttribute("aria-selected", "true");
});

test("ARC-24 folds: ← → and e, a click on the triangle, the folded mark, kept across restarts", async ({
  page,
}) => {
  await allSessions(page);
  // On a session, ← goes out to its project, then folds it.
  await page.keyboard.press("ArrowLeft");
  const muster = line(page, "muster");
  await expect(muster).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowLeft");
  await expect(muster).toHaveAttribute("aria-expanded", "false");
  // Folded, it says how the most urgent of what it hides stands: asking first.
  await expect(muster).toContainText("5 folded");
  await expect(muster.getByRole("img", { name: "needs you" })).toBeVisible();
  await expect(line(page, /^Flaky resize/)).toHaveCount(0);
  await page.keyboard.press("ArrowRight");
  await expect(muster).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowRight");
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  // e on a session folds its project, which is then selected.
  await page.keyboard.press("e");
  await expect(muster).toHaveAttribute("aria-selected", "true");
  await expect(muster).toHaveAttribute("aria-expanded", "false");
  // A project folds by a click on its triangle.
  await line(page, "lamphaus").locator(".mu-tree-fold").click();
  await expect(line(page, "lamphaus")).toContainText("3 folded");
  await expect(line(page, "lamphaus").getByRole("img", { name: "needs you" })).toBeVisible();
  // The folds are the window's own, kept across a restart.
  await page.reload();
  await page.keyboard.press("Meta+1");
  await expect(line(page, "muster")).toHaveAttribute("aria-expanded", "false");
  await expect(line(page, "lamphaus")).toHaveAttribute("aria-expanded", "false");
});

test("ARC-24 out of reach, and no projects yet", async ({ page }) => {
  await allSessions(page);
  await page.evaluate(() =>
    window.__muster.setLink({ state: "down", reason: "the daemon is not running", since: 0 }),
  );
  // The window's banner says it, with the daemon's reason (src/flows/Away.tsx).
  await expect(page.getByRole("alert")).toContainText("Muster's daemon is out of reach.");
  await expect(page.getByRole("alert")).toContainText("the daemon is not running");
  await expect(line(page, "muster")).toBeVisible();
  await page.evaluate(() => {
    const mock = window.__muster;
    mock.world.projects = [];
    mock.world.workspaces = [];
    mock.world.sessions = [];
    mock.publish({ reset: true });
  });
  // With no projects, the window's first run takes the lists of sessions (Welcome.html).
  await expect(page.getByRole("heading", { name: "Add a project to start" })).toBeVisible();
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
  // In a workspace with none, one is started for it, beside its chat and on no line of its own.
  await line(page, /^Flaky resize/).click();
  await page.getByRole("button", { name: "Shell", exact: true }).click();
  await expect(region(page, "Shell")).toContainText("zsh");
  await expect(line(page, "Shell: zsh")).toHaveCount(0);
  const started = await page.evaluate(() =>
    window.__muster.sent.some(
      (command) => typeof command === "object" && "StartSession" in command,
    ),
  );
  expect(started).toBe(true);
  // From a project's line, the shell opens in its checkout, beside the chat there.
  await page.keyboard.press("Shift+Q");
  await line(page, "muster").click();
  await page.keyboard.press("T");
  await expect(line(page, /^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
  await expect(region(page, "Shell")).toBeVisible();
});

test("PRD-15 a plugin's panel opens beside the session on its key, and its x puts it away", async ({
  page,
}) => {
  await allSessions(page);
  await page.keyboard.press("G");
  const graph = region(page, "Session graph");
  await expect(graph).toContainText("graph, from the plugin");
  await expect(line(page, "Panel: Session graph")).toHaveCount(0);
  // Another session of another worktree shows none.
  await line(page, /^Flaky resize/).click();
  await expect(graph).toHaveCount(0);
  await line(page, /^Split view\.rs/).click();
  await graph.getByRole("button", { name: "Put the graph away" }).click();
  await expect(graph).toHaveCount(0);
  // A project's line has no session to open it beside.
  await line(page, "muster").click();
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
  const separator = page.getByRole("separator", { name: /^Between/ });
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
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await expect(sidebar).toBeHidden();
  await expect(page.getByRole("tree")).toBeHidden();
  // Its head stays at the window's top.
  const title = region(page, "Split view.rs by responsibility");
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
  // f again, and the sidebar's button in the head gives the window back.
  await page.keyboard.press("f");
  await page.keyboard.press("Shift+Q");
  await page.getByRole("button", { name: "Show the sidebar" }).click();
  await expect(sidebar).toBeVisible();
  // On a project's line there is no session to fill the window with.
  await line(page, "muster").click();
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

test("PRD §5 by hand: c the checks and v a review open with the keys, o a draft pull request, b it in the browser", async ({
  page,
}) => {
  await allSessions(page);
  await line(page, /^Upgrade AGP/).click();
  const bar = page.locator(".mu-statusbar");
  await page.keyboard.press("b");
  await expect(bar).toContainText("No pull request from this branch is known; o opens one.");
  await page.keyboard.press("o");
  await expect(bar).toContainText(
    "Draft pull request #120 is open: https://github.com/o/lamphaus/pull/120",
  );
  await expect(region(page, "Upgrade AGP to 9.1")).toContainText("#120");
  await page.keyboard.press("b");
  await expect
    .poll(() => page.evaluate(() => window.__muster.openedUrls))
    .toEqual(["https://github.com/o/lamphaus/pull/120"]);

  // The checks, a session of their own, have the keys as they start.
  await page.keyboard.press("c");
  await expect(line(page, /^Checks/)).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("heading", { name: "Checks" })).toBeVisible();
  await expect(bar).toContainText("give the keys back");
  await page.keyboard.press("Shift+Q");
  await line(page, /^Upgrade AGP/).click();
  await page.keyboard.press("v");
  await expect(line(page, /^Review/)).toHaveAttribute("aria-selected", "true");

  // On a project's line, where none of them can be.
  await page.keyboard.press("Shift+Q");
  await line(page, "lamphaus").click();
  await page.keyboard.press("c");
  await expect(bar).toContainText("Select a session in a worktree to run the checks there.");
});

test("PRD §5 the palette offers the stages by hand on a session, as its keys do", async ({
  page,
}) => {
  await allSessions(page);
  await line(page, /^Flaky resize test/).click();
  await page.keyboard.press("Meta+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await expect(palette.getByRole("option", { name: /Run the project's checks/ })).toContainText(
    "c",
  );
  await expect(
    palette.getByRole("option", { name: /Have Claude Code review the change/ }),
  ).toContainText("v");
  await expect(
    palette.getByRole("option", { name: /Push the branch and open a draft pull request/ }),
  ).toContainText("o");
  await palette.getByRole("option", { name: /Open the pull request in the browser/ }).click();
  await expect
    .poll(() => page.evaluate(() => window.__muster.openedUrls))
    .toEqual(["https://github.com/FurkanCodes/muster/pull/88"]);
  await page.keyboard.press("Meta+k");
  await palette.getByRole("option", { name: /Run the project's checks/ }).click();
  await expect(page.getByRole("heading", { name: "Checks" })).toBeVisible();
});

test("ARC-25 3,000 sessions: a few dozen lines drawn, each where it belongs, the wheel and the keys smooth", async ({
  page,
}) => {
  await allSessions(page);
  await page.evaluate(() => {
    const mock = window.__muster;
    const base = mock.world.sessions[0];
    if (!base) return;
    const many = Array.from({ length: 3000 }, (_, n) => ({
      ...base,
      id: `01JE${String(n).padStart(22, "0")}`,
      title: `Session ${n}`,
    }));
    mock.world.sessions.push(...many);
    mock.publish({ sessions: many });
  });
  const tree = page.getByRole("tree", { name: "Projects and sessions" });
  await expect(line(page, /^Session 0,/)).toBeVisible();
  const items = tree.getByRole("treeitem");
  expect(await items.count()).toBeLessThan(120);
  // The lines drawn follow one another without a gap or an overlap.
  const gaps = () =>
    tree.locator("[data-line]").evaluateAll((nodes) =>
      nodes.slice(1).map((node, at) => {
        const above = nodes[at]?.getBoundingClientRect();
        return Math.round(node.getBoundingClientRect().top - (above?.bottom ?? 0));
      }),
    );
  expect(new Set(await gaps())).toEqual(new Set([0]));

  // The wheel to the end: the last lines are drawn there, and only those near.
  await tree.hover();
  for (let turn = 0; turn < 40; turn += 1) await page.mouse.wheel(0, 5000);
  await expect(line(page, /^Session 2999,/)).toBeInViewport();
  expect(await items.count()).toBeLessThan(120);
  expect(new Set(await gaps())).toEqual(new Set([0]));
  await line(page, /^Session 2999,/).click();
  await page.keyboard.press("k");
  await expect(line(page, /^Session 2998,/)).toHaveAttribute("aria-selected", "true");
  await expect(line(page, /^Session 2998,/)).toBeInViewport();

  // One selected for the person far above is scrolled to and drawn.
  await page.evaluate(
    (session) =>
      (
        window as unknown as { __musterUi: { select(screen: string, id: string): void } }
      ).__musterUi.select("all", session),
    id(104),
  );
  await expect(line(page, /^Resize settles once on release/)).toBeInViewport();
  expect(await items.count()).toBeLessThan(120);
});
