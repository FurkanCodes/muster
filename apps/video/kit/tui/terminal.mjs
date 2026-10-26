// Muster's terminal UI in a real terminal, for the recorder: the `muster`
// binary in a pseudo-terminal (bridge.py), drawn by xterm.js in headless
// Chromium (term.html) at twice the size, and driven by key presses as a
// person would. Screenshots and where text is on the screen come from the
// same page.

import { spawn } from "node:child_process";
import { createReadStream, existsSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(here, "../..");
const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
};

export const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

/** Opens a terminal of `width` by `height` pixels running `muster` in `cwd`. */
export async function openTerminal(world, cwd, { width = 1440, height = 900, font = 17, args = [] } = {}) {
  const server = createServer((request, response) => {
    const file = join(root, decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname));
    if (!file.startsWith(root) || !existsSync(file) || statSync(file).isDirectory()) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
    createReadStream(file).pipe(response);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
  page.on("pageerror", (error) => console.error("terminal page:", error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/kit/tui/term.html?width=${width}&height=${height}&font=${font}`);
  await page.waitForFunction(() => window.ready === true);
  const size = await page.evaluate(() => window.size());
  const bridge = spawn(
    "python3",
    ["-I", join(here, "bridge.py"), String(size.columns), String(size.rows), String(size.width), String(size.height), "muster", ...args],
    { env: world.env, cwd },
  );
  // The program's bytes reach the terminal in order, each write drawn
  // before the next is given.
  let drawn = Promise.resolve();
  let last = Date.now();
  bridge.stdout.on("data", (chunk) => {
    last = Date.now();
    const base64 = chunk.toString("base64");
    drawn = drawn.then(() => page.evaluate((data) => window.feed(data), base64));
  });
  bridge.stderr.on("data", (data) => console.error("bridge:", String(data)));
  await page.exposeFunction("reply", (data) => bridge.stdin.write(Buffer.from(data, "utf8")));

  return {
    page,
    size,
    /** Waits until the program has drawn, and then been still for `quiet` ms. */
    async settle(quiet = 400, most = 6000) {
      const start = Date.now();
      while (Date.now() - start < most) {
        await drawn;
        if (Date.now() - last >= quiet) return;
        await sleep(50);
      }
    },
    /** Presses `keys` one after another, as named for Playwright (`Enter`, `Shift+Tab`, `n`). */
    async press(...keys) {
      for (const key of keys) {
        await page.keyboard.press(key);
        await sleep(120);
      }
    },
    async type(text) {
      await page.keyboard.type(text, { delay: 25 });
    },
    text: () => page.evaluate(() => window.screenText()),
    /** Waits until `text` is on the screen; where it is. */
    async waitFor(text, most = 15000) {
      const start = Date.now();
      while (Date.now() - start < most) {
        await drawn;
        const found = await page.evaluate((what) => window.find(what), text);
        if (found) return found;
        await sleep(100);
      }
      throw new Error(`"${text}" never showed; the screen:\n${await page.evaluate(() => window.screenText())}`);
    },
    /** Waits until `text` has gone from the screen. */
    async waitGone(text, most = 15000) {
      const start = Date.now();
      while (Date.now() - start < most) {
        await drawn;
        if (!(await page.evaluate((what) => window.find(what), text))) return;
        await sleep(100);
      }
      throw new Error(`"${text}" never went`);
    },
    /** A box in pixels: from `row`, `column` for `rows` rows and `columns` columns. */
    box(row, column, rows, columns) {
      const { cell } = size;
      const round = (value) => Math.round(value * 2) / 2;
      return { x: round(column * cell.width), y: round(row * cell.height), w: round(columns * cell.width), h: round(rows * cell.height) };
    },
    find: (text, from) => page.evaluate(([what, row]) => window.find(what, row), [text, from ?? 0]),
    async screenshot(path) {
      await drawn;
      await page.screenshot({ path, clip: { x: 0, y: 0, width: size.width, height: size.height } });
    },
    async close() {
      bridge.kill();
      await browser.close();
      server.close();
    },
  };
}
