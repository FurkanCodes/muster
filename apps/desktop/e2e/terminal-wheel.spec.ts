import { expect, type Page, test } from "@playwright/test";

// The wheel over a session's terminal, against the mock daemon (its bridge
// keeps everything typed, `typed`). A program that takes the mouse, as
// Claude Code does on its full-screen views, is sent a report for every
// line the wheel goes, where the pointer is; one that does not is sent
// nothing, and the terminal scrolls its own lines.

type Mock = {
  typed: { session: string; text: string }[];
  world: { sessions: { id: string; title: string }[] };
  output: (session: string, text: string) => void;
  setModes: (session: string, modes: Record<string, unknown>) => void;
};

const typedTo = (page: Page, session: string) =>
  page.evaluate(
    (id) =>
      (window as unknown as { __muster: Mock }).__muster.typed
        .filter((typed) => typed.session === id)
        .map((typed) => typed.text),
    session,
  );

/** The terminal's grid on the page: where its cells are, and how tall each is. */
async function grid(page: Page) {
  const screen = page.locator(".xterm-screen").first();
  const box = await screen.boundingBox();
  if (!box) throw new Error("the terminal is not drawn");
  const { cols, rows } = await page.evaluate(() => {
    const xterms = (
      window as unknown as { __musterXterms: Record<string, { cols: number; rows: number }> }
    ).__musterXterms;
    const [xterm] = Object.values(xterms);
    return { cols: xterm?.cols ?? 0, rows: xterm?.rows ?? 0 };
  });
  return { box, cols, rows, cell: { width: box.width / cols, height: box.height / rows } };
}

async function openSession(page: Page, title: string): Promise<string> {
  await page.goto("/");
  const sidebar = page.getByRole("navigation", { name: "Sessions" });
  await sidebar.getByRole("button", { name: /^All/ }).click();
  await sidebar.getByRole("treeitem", { name: new RegExp(`^${title}`) }).click();
  await expect(page.locator(".xterm")).toHaveCount(1);
  await page.waitForTimeout(300);
  return page.evaluate(
    (wanted) =>
      (window as unknown as { __muster: Mock }).__muster.world.sessions.find(
        (session) => session.title === wanted,
      )?.id ?? "",
    title,
  );
}

test("ARC-24 a program that takes the mouse is sent every line a trackpad goes, where the pointer is", async ({
  page,
}) => {
  const id = await openSession(page, "Sketch the Tauri shell");
  // Claude Code's full screen: the alternate screen, every movement, SGR.
  await page.evaluate((session) => {
    const bridge = (window as unknown as { __muster: Mock }).__muster;
    bridge.output(session, "\x1b[?1049h\x1b[?1003h\x1b[?1006h");
    bridge.setModes(session, {
      mouse: "any_motion",
      mouse_encoding: "sgr",
      alternate_screen: true,
    });
  }, id);
  const { box, cell } = await grid(page);
  // Over the cell in column 11, row 6.
  await page.mouse.move(box.x + cell.width * 10.5, box.y + cell.height * 5.5);
  const before = (await typedTo(page, id)).length;

  // A trackpad's small steps: four pixels at a time, six lines in all.
  const steps = Math.ceil((cell.height * 6) / 4);
  for (let step = 0; step < steps; step += 1) await page.mouse.wheel(0, 4);
  await expect
    .poll(async () => (await typedTo(page, id)).slice(before).join(""))
    .toBe("\x1b[<65;11;6M".repeat(6));

  // A flick up goes its whole way at once, not one line an event.
  const flicked = (await typedTo(page, id)).length;
  await page.mouse.wheel(0, -(cell.height * 5 + 1));
  await expect
    .poll(async () => (await typedTo(page, id)).slice(flicked))
    .toEqual(["\x1b[<64;11;6M".repeat(5)]);
});

test("ARC-24 a program that does not take the mouse is sent nothing by the wheel", async ({
  page,
}) => {
  const id = await openSession(page, "Sketch the Tauri shell");
  const before = (await typedTo(page, id)).length;
  const { box, cell } = await grid(page);
  await page.mouse.move(box.x + cell.width * 4, box.y + cell.height * 4);
  for (let step = 0; step < 10; step += 1) await page.mouse.wheel(0, -cell.height);
  await page.waitForTimeout(200);
  expect((await typedTo(page, id)).slice(before)).toEqual([]);
});
