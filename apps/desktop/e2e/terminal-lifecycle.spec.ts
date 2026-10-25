import { expect, type Page, test } from "@playwright/test";
import type { MockBridge } from "../src/ipc/mock/bridge";

// A session's terminal kept between its showings, end to end over the mock
// daemon in WebKit: moving it (alone or beside its shell, the changes and
// back, Tickets and back, another session and back) never makes it afresh
// (`__musterXtermsMade`, src/terminal/testing.ts) nor replays what it holds,
// and it takes nothing but its own session's stream. src/terminal/kept.ts.

declare global {
  interface Window {
    __muster: MockBridge;
  }
}

const line = (page: Page, name: string | RegExp) =>
  page.getByRole("treeitem", { name, exact: typeof name === "string" });

async function allSessions(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("listbox", { name: "Needs you sessions" })).toBeVisible();
  await page.keyboard.press("Meta+1");
  await expect(page.getByRole("tree", { name: "Projects and sessions" })).toBeVisible();
}

/** How many xterms each session's terminal has been given. */
const made = (page: Page) =>
  page.evaluate(() => ({
    ...(window as unknown as { __musterXtermsMade?: Record<string, number> }).__musterXtermsMade,
  }));

/** What a session's terminal shows. */
const shown = (page: Page, session: string) =>
  page.evaluate(
    (session) =>
      (
        window as unknown as { __musterTerminals?: Record<string, () => string> }
      ).__musterTerminals?.[session]?.() ?? "",
    session,
  );

/** The chats of the mock's working day, by title, as the All list shows them. */
const chats = (page: Page) =>
  page.evaluate(() =>
    window.__muster.world.sessions
      .filter((one) => one.harness !== "shell" && !one.harness.startsWith("plugin:"))
      .map((one) => ({ id: one.id, title: one.title })),
  );

async function open(page: Page, title: string) {
  await line(page, new RegExp(`^${title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`)).click();
  await expect(page.getByRole("heading", { name: title })).toBeVisible();
  await expect(page.locator(".xterm").first()).toBeVisible();
}

const write = (page: Page, session: string, text: string) =>
  page.evaluate(([id, words]) => window.__muster.output(id, words), [session, text] as const);

test("ARC-25 sessions shown in turn keep each its own terminal, made once, with only its own output", async ({
  page,
}) => {
  await allSessions(page);
  const [a, b] = await chats(page);
  if (!a || !b) throw new Error("the working day has two chats");
  await open(page, a.title);
  await write(page, a.id, "\r\nonly-in-a\r\n");
  // B's output is there before B is shown: its attach replays it at once,
  // in the same commit as A goes, and A's kept terminal must not take it.
  await write(page, b.id, "\r\nonly-in-b\r\n");
  await open(page, b.title);
  await expect.poll(() => shown(page, b.id)).toContain("only-in-b");
  expect(await shown(page, a.id)).not.toContain("only-in-b");
  // Written while it is away: shown as it comes back, once.
  await write(page, a.id, "\r\nwhile-a-was-away\r\n");
  for (const title of [a.title, b.title, a.title]) await open(page, title);
  await expect.poll(() => shown(page, a.id)).toContain("while-a-was-away");
  const aText = await shown(page, a.id);
  expect(aText.split("while-a-was-away")).toHaveLength(2);
  expect(aText.split("only-in-a")).toHaveLength(2);
  expect(aText).not.toContain("only-in-b");
  expect(await shown(page, b.id)).not.toContain("only-in-a");
  const counts = await made(page);
  expect(counts[a.id]).toBe(1);
  expect(counts[b.id]).toBe(1);
  // Each came back from where it was, not from the start.
  const attaches = await page.evaluate(() => window.__muster.attaches);
  const again = attaches.filter((one) => one.session === a.id);
  expect(again[0]?.fromSeq).toBeNull();
  expect(again.slice(1).every((one) => one.fromSeq !== null)).toBe(true);
});

test("ARC-25 the agent's terminal stays itself as its shell is placed beside it and taken off", async ({
  page,
}) => {
  await allSessions(page);
  await open(page, "Sketch the Tauri shell");
  const agent = await page.evaluate(
    () => window.__muster.world.sessions.find((one) => one.title === "Sketch the Tauri shell")?.id,
  );
  if (!agent) throw new Error("no agent");
  const before = (await made(page))[agent];
  await page.keyboard.press("T");
  const shell = page.getByRole("region", { name: "Shell", exact: true });
  await expect(shell).toBeVisible();
  await page.keyboard.press("Shift+Q");
  await shell.getByRole("button", { name: "Take the shell off the screen" }).click();
  await expect(shell).toHaveCount(0);
  expect((await made(page))[agent]).toBe(before);
});

test("ARC-25 the agent's terminal stays itself through the changes and Tickets, and back", async ({
  page,
}) => {
  await allSessions(page);
  const [a] = await chats(page);
  if (!a) throw new Error("no chat");
  await open(page, a.title);
  const before = (await made(page))[a.id];
  await page.getByRole("button", { name: "Changes", exact: true }).click();
  await expect(page.locator(".xterm")).toHaveCount(0);
  await page.getByRole("button", { name: "Changes", exact: true }).click();
  await expect(page.locator(".xterm").first()).toBeVisible();
  await page
    .getByRole("navigation", { name: "Sessions" })
    .getByRole("button", { name: /^Tickets/ })
    .click();
  await expect(page.locator(".xterm")).toHaveCount(0);
  await page.keyboard.press("Meta+1");
  await expect(page.locator(".xterm").first()).toBeVisible();
  expect((await made(page))[a.id]).toBe(before);
});

test("ARC-25 a frame of a stream let go of never reaches the terminal", async ({ page }) => {
  await allSessions(page);
  const [a, b] = await chats(page);
  if (!a || !b) throw new Error("the working day has two chats");
  await open(page, a.title);
  await open(page, b.title);
  await page.evaluate((id) => window.__muster.lateFrame(id, "\r\nlate-frame\r\n"), a.id);
  // Neither A's terminal, kept, nor B's, on screen as it comes.
  expect(await shown(page, b.id)).not.toContain("late-frame");
  await open(page, a.title);
  await page.waitForTimeout(100);
  expect(await shown(page, a.id)).not.toContain("late-frame");
  expect(await shown(page, b.id)).not.toContain("late-frame");
});

test("ARC-25 the terminals kept are bounded, the least lately shown let go first", async ({
  page,
}) => {
  await allSessions(page);
  const all = await chats(page);
  // The one shown and four idle are kept: the sixth shown lets the first go.
  const six = all.slice(0, 6);
  expect(six).toHaveLength(6);
  for (const chat of six) await open(page, chat.title);
  const [first, second] = six;
  if (!first || !second) throw new Error("no chats");
  await open(page, first.title);
  // The second, now the least lately shown, went as the first came back.
  await open(page, second.title);
  // The latest shown before them is kept still: back on screen, made once.
  const [, , , , , sixth] = six;
  if (!sixth) throw new Error("no sixth chat");
  await open(page, sixth.title);
  const counts = await made(page);
  expect(counts[first.id]).toBe(2);
  expect(counts[second.id]).toBe(2);
  for (const chat of six.slice(2)) expect(counts[chat.id]).toBe(1);
});

test("ARC-25 a removed session's kept terminal is let go of", async ({ page }) => {
  await allSessions(page);
  const [a, b] = await chats(page);
  if (!a || !b) throw new Error("the working day has two chats");
  await open(page, a.title);
  await open(page, b.title);
  const kept = () =>
    page.evaluate(
      (id) =>
        Boolean(
          (window as unknown as { __musterXterms?: Record<string, unknown> }).__musterXterms?.[id],
        ),
      a.id,
    );
  expect(await kept()).toBe(true);
  await page.evaluate((id) => window.__muster.removeSession(id), a.id);
  await expect.poll(kept).toBe(false);
});

test("ARC-25 a terminal the layout has no room for streams nothing, and comes back as it was", async ({
  page,
}) => {
  await allSessions(page);
  await open(page, "Sketch the Tauri shell");
  const agent = await page.evaluate(
    () => window.__muster.world.sessions.find((one) => one.title === "Sketch the Tauri shell")?.id,
  );
  if (!agent) throw new Error("no agent");
  await page.keyboard.press("T");
  await expect(page.getByRole("region", { name: "Shell", exact: true })).toBeVisible();
  const before = (await made(page))[agent];
  // Too narrow for both: the shell, with the keys, opens over the agent, laid out at 0 by 0.
  await page.setViewportSize({ width: 760, height: 700 });
  await expect.poll(() => page.evaluate((id) => window.__muster.attached(id), agent)).toBe(false);
  const sized = await page.evaluate(() => window.__muster.sized);
  expect(sized.every((size) => size.cols > 0 && size.rows > 0)).toBe(true);
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect.poll(() => page.evaluate((id) => window.__muster.attached(id), agent)).toBe(true);
  expect((await made(page))[agent]).toBe(before);
});
