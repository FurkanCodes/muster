// Records the kit's shots: the desktop app's real window, built over its
// pretend daemon (`pnpm build:mock` in apps/desktop) and driven through the
// story in headless Chromium. Each shot is a 2x screenshot; each crop and
// each point a click lands on is measured from the page itself, in the
// window's pixels, and written to crops.js. Nothing here reaches a real
// daemon, tracker or agent: the pretend world is changed only through
// window.__muster, as the desktop app's own end-to-end tests change it.

import { execFileSync } from "node:child_process";
import { copyFileSync, createReadStream, existsSync, mkdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = fileURLToPath(new URL(".", import.meta.url));
const desktop = resolve(here, "../../desktop");
const site = join(desktop, "dist-mock");
const out = join(here, "shots");

/** The window's size, as the video frames it; the list is shot taller once. */
const WINDOW = { width: 1440, height: 900 };
const TALL = { width: 1440, height: 1240 };
const SCALE = 2;

const id = (n) => `01JD${String(n).padStart(22, "0")}`;
const CODEX = id(101);
const SHELL = id(102);

execFileSync("pnpm", ["build:mock"], { cwd: desktop, stdio: "inherit" });

const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".json": "application/json",
};
const server = createServer((request, response) => {
  const path = decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname);
  let file = join(site, path);
  if (!file.startsWith(site) || !existsSync(file) || statSync(file).isDirectory())
    file = join(site, "index.html");
  response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(response);
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const address = server.address();
const url = `http://127.0.0.1:${typeof address === "object" && address ? address.port : 0}/`;

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
// Kep, the app's icon.
copyFileSync(join(desktop, "src-tauri/icons/icon.png"), join(out, "kep.png"));

// WebGL off: headless Chromium draws xterm's WebGL canvas blank, and the
// terminal falls back to its DOM renderer, as the app does without a GPU.
const browser = await chromium.launch({ args: ["--disable-webgl", "--disable-3d-apis"] });
const page = await browser.newPage({ viewport: WINDOW, deviceScaleFactor: SCALE });

const shots = {};
const crops = {};
const points = {};

/** A box on the page, in window pixels, as a crop of the shot about to be taken. */
async function box(locator, closest) {
  const found = await locator.first().evaluate(
    (element, selector) => {
      const target = selector ? (element.closest(selector) ?? element) : element;
      const rect = target.getBoundingClientRect();
      return { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
    },
    closest ?? null,
  );
  const round = (value) => Math.round(value * 2) / 2;
  return { x: round(found.x), y: round(found.y), w: round(found.w), h: round(found.h) };
}

/**
 * Saves the window as it is now, as `name`, with the crops and points
 * measured on it: `crop: { name: [locator, closest?] }`, `point: { name: locator }`.
 */
async function shot(name, { crop = {}, point = {} } = {}) {
  await page.waitForTimeout(700);
  for (const [key, spec] of Object.entries(crop)) {
    // A crop is one element's box, the box spanning two (`union`), or
    // edges taken from different elements (`left`, `top`, `right`, `bottom`).
    if (spec.left) {
      const edge = async (part) => box(...part);
      const left = (await edge(spec.left)).x;
      const top = (await edge(spec.top)).y;
      const right = await edge(spec.right);
      const bottom = await edge(spec.bottom);
      crops[key] = { shot: name, x: left, y: top, w: right.x + right.w - left, h: bottom.y + bottom.h - top };
      continue;
    }
    const parts = spec.union ?? [spec];
    const boxes = [];
    for (const [locator, closest] of parts) boxes.push(await box(locator, closest));
    const x = Math.min(...boxes.map((b) => b.x));
    const y = Math.min(...boxes.map((b) => b.y));
    const w = Math.max(...boxes.map((b) => b.x + b.w)) - x;
    const h = Math.max(...boxes.map((b) => b.y + b.h)) - y;
    crops[key] = { shot: name, x, y, w, h };
  }
  for (const [key, locator] of Object.entries(point)) {
    const found = await box(locator);
    points[key] = { shot: name, x: Math.round(found.x + found.w / 2), y: Math.round(found.y + found.h / 2) };
  }
  const file = `${name}.png`;
  await page.screenshot({ path: join(out, file) });
  const size = page.viewportSize();
  shots[name] = { file, width: size.width, height: size.height };
  console.log(`shot ${file}`);
}

const muster = (fn, arg) => page.evaluate(fn, arg);

/**
 * The "Needs you: …" badge at the start of a session's banner: the banner
 * from its left edge to the end of what the agent asks, without the empty
 * middle and the keys at its end.
 */
async function badge(name, activity, shotName) {
  const found = await page
    .getByText("answer in the terminal")
    .first()
    .evaluate((element, asked) => {
      const banner = element.closest("[class*=asks], [class*=banner], [class*=needs]") ?? element.parentElement;
      // The words themselves, as a range over the text that holds them.
      const walker = document.createTreeWalker(banner, NodeFilter.SHOW_TEXT);
      let node = walker.nextNode();
      while (node && !node.data.includes(asked)) node = walker.nextNode();
      const range = document.createRange();
      const start = node.data.indexOf(asked);
      range.setStart(node, start);
      range.setEnd(node, start + asked.length);
      const outer = banner.getBoundingClientRect();
      const inner = range.getBoundingClientRect();
      return { x: outer.x, y: outer.y, w: inner.right - outer.x + 12, h: outer.height };
    }, activity);
  crops[name] = { shot: shotName, ...found };
}
const sidebar = page.getByRole("navigation").first();
const line = (title) => [sidebar.getByText(title), "[role=treeitem]"];
const terminal = [page.locator(".xterm-screen"), null];

/** Codex's newest turn in the desktop worktree, as the diff pane shows it. */
const LINK_DIFF = [
  "diff --git a/crates/muster-desktop/src/link.rs b/crates/muster-desktop/src/link.rs",
  "--- a/crates/muster-desktop/src/link.rs",
  "+++ b/crates/muster-desktop/src/link.rs",
  "@@ -1,9 +1,24 @@",
  " //! The desktop app's link to the daemon.",
  "-use muster_client::Connection;",
  "+use muster_client::{Connection, Received};",
  "+use tauri::ipc::Channel;",
  " ",
  " pub struct Link {",
  "     connection: Connection,",
  "+    state: Channel<StateMessage>,",
  " }",
  " ",
  " impl Link {",
  "-    // TODO: deltas",
  "+    /// Subscribes once, then hands each change to the webview.",
  "+    pub async fn follow(mut self) -> Result<(), LinkError> {",
  "+        let snapshot = self.connection.subscribe().await?;",
  "+        self.state.send(StateMessage::Reset(snapshot))?;",
  "+        while let Some(received) = self.connection.next().await {",
  "+            match received? {",
  "+                Received::Delta(delta) => self.state.send(StateMessage::Patch(delta))?,",
  "+                Received::Closed => return Ok(()),",
  "+            }",
  "+        }",
  "+        Ok(())",
  "+    }",
  " }",
  "",
].join("\n");

await page.goto(url);
await page.getByText("Split view.rs by responsibility").first().waitFor();
await muster(
  ([codex, shell]) => {
    const m = window.__muster;
    m.setLink({ state: "up", version: "0.7.4", protocol: 58 });
    // Codex is still at work as the story starts; it asks later.
    m.updateSession(codex, {
      status: { state: "running" },
      activity: { text: "editing src/link.rs", since: Date.now() },
    });
    m.emitSession(codex);
    // The shell moves to the checkout: at work in Codex's worktree, it
    // would hold off putting a turn back there. It runs `muster ssh` later.
    m.updateSession(shell, { workspace: "01JD0000000000000000000010", title: "zsh" });
    m.emitSession(shell);
  },
  [CODEX, SHELL],
);

// The list, tall enough for every row, then Gemini's question.
await page.setViewportSize(TALL);
await shot("rows", {
  crop: {
    "row-split": line("Split view.rs by responsibility"),
    "row-flaky": line("Flaky resize test under load"),
    "row-agp": line("Upgrade AGP to 9.1"),
    "row-gemini": line("Share the Gradle cache across worktrees"),
  },
});
await sidebar.getByText("Share the Gradle cache across worktrees").first().click();
await page.getByText("answer in the terminal").first().waitFor();
await badge("badge-gemini", "asks which cache folder to use", "gemini");
await shot("gemini", {
  crop: { "banner-gemini": [page.getByText("answer in the terminal"), "[class*=asks], [class*=banner], [class*=needs]"] },
});
await page.setViewportSize(WINDOW);
await sidebar.getByText("Split view.rs by responsibility").first().click();

// 1. Agents side by side: the list, then a new one started from a prompt.
await shot("list", {
  crop: {
    sidebar: [sidebar, null],
    "list-working": line("Split view.rs by responsibility"),
    "list-finished": line("Resize settles once on release"),
    "list-needs-you": [page.getByText(/^Needs you/), "button, [role=tab], [role=radio]"],
  },
  point: { "new-session": page.getByRole("button", { name: /New session/ }) },
});
await page.keyboard.press("n");
const startBox = page.getByRole("form", { name: "Start work" });
const prompt = startBox.getByRole("textbox", { name: "First prompt" });
await prompt.waitFor();
await startBox.getByRole("combobox", { name: "Where", exact: true }).click();
await page.getByRole("listbox", { name: "Where" }).getByRole("option").filter({ hasText: /new worktree/i }).first().click();
await prompt.click();
await shot("start-empty", { crop: { "start-box": [startBox, null], prompt: [prompt, null] } });
await prompt.fill("Add a dark mode to Settings");
const startButton = startBox.getByRole("button", { name: /^Start/ }).first();
await shot("start", {
  crop: { "start-box": [startBox, null], prompt: [prompt, null] },
  point: { start: startButton },
});
await startButton.click();
// As the real daemon names them from the prompt: the chat, and its branch.
await page.waitForTimeout(500);
await muster(() => {
  const m = window.__muster;
  const session = m.world.sessions.at(-1);
  m.updateWorkspace(session.workspace, {
    kind: { kind: "worktree", path: "~/.muster/worktrees/muster/dark-mode", branch: "muster/dark-mode-settings" },
  });
  m.updateSession(session.id, {
    title: "Add a dark mode to Settings",
    activity: { text: "reading Settings.tsx", since: Date.now() },
  });
  m.emitSession(session.id);
  m.output(
    session.id,
    "\x1b[38;5;208m✻\x1b[0m Add a dark mode to Settings\r\n\r\n\x1b[32m●\x1b[0m Read(src/screens/settings/Settings.tsx)\r\n  ⎿  Read 214 lines\r\n\r\n\x1b[32m●\x1b[0m I'll add a Look section with Light, Dark and System.\r\n\r\n\x1b[33m✻\x1b[0m Thinking…\r\n",
  );
});
await shot("started", {
  crop: {
    "row-new": line("Add a dark mode to Settings"),
    "branch-new": [sidebar.getByText("muster/dark-mode-settings"), ".sessions-line, [role=treeitem]"],
    sidebar: [sidebar, null],
  },
});

// 2. One asks: its line says so, the window is told, a click answers.
await muster((session) => {
  const m = window.__muster;
  m.updateSession(session, { activity: { text: "wants to run cargo build", since: Date.now() } });
  m.setStatus(session, { state: "needs_you", reason: "permission" }, true);
}, CODEX);
const codexLine = sidebar.getByText("Sketch the Tauri shell").first();
await shot("asking", {
  crop: { "row-codex-asking": line("Sketch the Tauri shell"), "needs-you-tab": [page.getByText(/^Needs you/), "button, [role=tab], [role=radio]"], sidebar: [sidebar, null] },
  point: { codex: codexLine },
});
await codexLine.click();
await page.getByText("answer in the terminal").first().waitFor();
await badge("badge-codex", "wants to run cargo build", "question");
await shot("question", {
  crop: {
    "banner-codex": [page.getByText("answer in the terminal"), "[class*=asks], [class*=banner], [class*=needs]"],
    "terminal-question": terminal,
  },
});
await muster((session) => {
  const m = window.__muster;
  m.output(session, "\r\n\r\n\x1b[32m✔\x1b[0m You approved: cargo build -p muster-desktop\r\n   Compiling muster-desktop v0.7.4\r\n");
  m.updateSession(session, { activity: { text: "running cargo build", since: Date.now() } });
  m.setStatus(session, { state: "running" });
}, CODEX);
await shot("answered", { crop: { "row-codex-answered": line("Sketch the Tauri shell"), "terminal-answered": terminal } });
// Its turn ends: only now can a turn be put back (never while it works).
await muster(
  ([session, diff]) => {
    const m = window.__muster;
    m.output(session, "    Finished `dev` profile in 9.81s\r\n\r\n\x1b[32m•\x1b[0m Built. The window now streams state through the link.\r\n");
    m.updateSession(session, { activity: null });
    m.setStatus(session, { state: "finished" });
    m.world.entryDiffs["01JD0000000000000000000011"][4] = { text: diff, cut: false };
  },
  [CODEX, LINK_DIFF],
);
await shot("finished", {
  crop: { "row-codex-finished": line("Sketch the Tauri shell"), "terminal-finished": terminal },
  point: { changes: page.getByRole("button", { name: "Changes", exact: true }) },
});

// 3. What changed: the turns, a collision, a turn put back.
await page.getByRole("button", { name: "Changes", exact: true }).click();
const turnsTab = page.getByRole("button", { name: "Turns 4" });
await turnsTab.click();
const turns = page.getByRole("listbox", { name: "Turns" });
const turn = turns.getByRole("option").first();
await turn.waitFor();
const overlap = { union: [[page.getByText("Git could merge them."), null], [page.getByText("See its changes"), null]] };
// The session's title line: its name, branch, what its work comes to, and its pull request.
const header = { union: [[page.getByRole("heading", { name: "Sketch the Tauri shell" }), null], [page.getByText("#91"), null]] };
await shot("turns", {
  crop: { overlap, "turns-before": [turns, null], "header-before": header },
  point: { turn },
});
await turn.click();
await shot("turn-diff", {
  crop: { diff: [page.getByRole("region", { name: "Diff" }), null] },
  point: { "put-back": page.getByRole("button", { name: "Put back", exact: true }) },
});
await page.getByRole("button", { name: "Put back", exact: true }).click();
const putBack = page.getByRole("dialog").getByRole("button", { name: "Put back the files" });
await putBack.waitFor();
await shot("put-back", {
  crop: { "dialog-put-back": [page.getByRole("dialog"), null] },
  point: { "put-back-files": putBack },
});
await putBack.click();
await page.getByRole("dialog").waitFor({ state: "detached" });
await shot("put-back-done", { crop: { "turns-after": [turns, null], "header-after": header, "put-back-line": [turns.getByRole("option").first(), null] } });

// 4. A ticket to an agent, and the pull requests.
await page.keyboard.press("Escape");
await page.keyboard.press("Escape");
await page.getByRole("button", { name: /^Tickets/ }).first().click();
const ticket = page.getByText("Desktop app: sessions list and one live terminal").first();
await ticket.waitFor();
await ticket.click();
const assign = page.getByRole("button", { name: /^Assign/ }).last();
await assign.waitFor();
await shot("tickets", {
  crop: {
    "ticket-list": { union: [[page.getByRole("heading", { name: "Tickets", exact: true }), null], [page.getByText("Move the docs to the new domain"), "[role=option]"]] },
    "ticket-detail": { union: [[page.getByRole("heading", { name: "Desktop app: sessions list and one live terminal" }), null], [assign, null]] },
    "ticket-line": [page.getByRole("option", { name: /#214/ }), null],
  },
  point: { assign },
});
await assign.click();
const assignDialog = page.getByRole("dialog", { name: /^Assign an agent/ });
await assignDialog.waitFor();
const startAssigned = assignDialog.getByRole("button", { name: /^Start/ }).first();
await shot("assign", { crop: { "dialog-assign": [assignDialog, null] }, point: { "start-assigned": startAssigned } });
await startAssigned.click();
await assignDialog.waitFor({ state: "detached" });
await muster(() => {
  const m = window.__muster;
  const session = m.world.sessions.at(-1);
  m.updateSession(session.id, { activity: { text: "reading src/app/App.tsx", since: Date.now() } });
  m.setStatus(session.id, { state: "running" });
  m.output(
    session.id,
    "\x1b[38;5;208m✻\x1b[0m Working on #214: Desktop app: sessions list and one live terminal\r\n\r\n\x1b[32m●\x1b[0m Read(src/app/App.tsx)\r\n  ⎿  Read 188 lines\r\n\r\n\x1b[32m●\x1b[0m The list comes from the daemon's snapshot; I'll stream the selected\r\n  session's terminal beside it.\r\n",
  );
});
await shot("assigned", {
  crop: { "row-ticket": line("#214 Desktop app"), "branch-ticket": [sidebar.getByText(/muster\/issue-214/), ".sessions-line, [role=treeitem]"] },
});
await sidebar.getByText("muster", { exact: true }).first().click();
await page.getByText("Where its work is").waitFor();
await shot("project", {
  crop: {
    "pr-approved": [page.getByText("#88"), "[class*=place-head], [class*=where-head], header, [class*=head]"],
    "pr-draft": [page.getByText("#91"), "[class*=place-head], [class*=where-head], header, [class*=head]"],
    where: [page.getByText("Where its work is"), "section"],
  },
});

// 5. The terminal beside it all: `muster ssh` to a machine, in Muster's own
// terminal, with the lines crates/muster/src/ssh.rs prints.
await muster((shell) => {
  const m = window.__muster;
  m.updateSession(shell, { title: "muster", activity: null });
  m.emitSession(shell);
  m.world.terminals[shell] = "";
}, SHELL);
await sidebar.getByText("muster", { exact: true }).first().click();
await sidebar.locator(".sessions-line").filter({ hasText: /^muster/ }).first().click().catch(() => {});
await page.evaluate((shell) => window.__muster.reveal(shell), SHELL);
await page.waitForTimeout(400);
await muster((shell) => {
  const m = window.__muster;
  m.output(
    shell,
    "\x1b[2J\x1b[H\x1b[35m❯\x1b[0m muster ssh vm\r\n" +
      "· Connecting to you@203.0.113.7…\r\n" +
      "· Connected (Linux x86_64): muster 0.7.4 is there.\r\n" +
      "· Opening a shell on vm…\r\n" +
      "\x1b[32myou@vm\x1b[0m:\x1b[34m~\x1b[0m$ muster claude \"Upgrade the test suite\"\r\n",
  );
}, SHELL);
const row = (text) => [page.locator(".xterm-rows > div").filter({ hasText: text }), null];
await shot("ssh", {
  crop: {
    "terminal-ssh": {
      left: row("muster ssh vm"),
      top: row("muster ssh vm"),
      right: [page.getByText("Connected (Linux x86_64): muster 0.7.4 is there."), null],
      bottom: row('muster claude "Upgrade the test suite"'),
    },
  },
});

const source = [
  "// Written by record.mjs: the kit's shots, and every crop and click point",
  "// on them, measured from the page, in window pixels (the PNGs are twice that).",
  `export const SCALE = ${SCALE};`,
  `export const shots = ${JSON.stringify(shots, null, 2)};`,
  `export const crops = ${JSON.stringify(crops, null, 2)};`,
  `export const points = ${JSON.stringify(points, null, 2)};`,
  "",
].join("\n");
writeFileSync(join(here, "crops.js"), source);
await browser.close();
server.close();
console.log(`${Object.keys(shots).length} shots, ${Object.keys(crops).length} crops, ${Object.keys(points).length} points`);
