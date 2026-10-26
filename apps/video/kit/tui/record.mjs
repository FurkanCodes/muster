// Records the terminal UI's shots: the real `muster` binary, its terminal
// UI in a real terminal (terminal.mjs) over a throwaway world (world.mjs),
// driven through the story by key presses. Each shot is a 2x screenshot;
// each crop is measured from where its text is on the terminal's grid,
// in the terminal's pixels, and written to crops.js with the shots.
//
//   node kit/tui/record.mjs [--text]   (--text also saves each screen as text)

import { copyFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { openTerminal, sleep } from "./terminal.mjs";
import { makeWorld } from "./world.mjs";

const here = fileURLToPath(new URL(".", import.meta.url));
const out = join(here, "shots");
const asText = process.argv.includes("--text");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
// Kep, the app's icon.
copyFileSync(join(here, "../../../desktop/src-tauri/icons/icon.png"), join(out, "kep.png"));

const ORANGE = "\x1b[38;5;208m";
const GREEN = "\x1b[32m";
const YELLOW = "\x1b[33m";
const DIM = "\x1b[2m";
const RESET = "\x1b[0m";
const say = (lines) => `${lines.join("\r\n")}\r\n`;

const world = makeWorld();
const shots = {};
const crops = {};
let term;

/** Saves the terminal as it is now, as `name`, with `measured` crops. */
async function shot(name, measured = {}) {
  await term.settle(450);
  for (const [key, box] of Object.entries(measured)) crops[key] = { shot: name, ...(await box) };
  await term.screenshot(join(out, `${name}.png`));
  if (asText) writeFileSync(join(out, `${name}.txt`), await term.text());
  shots[name] = { file: `${name}.png`, width: term.size.width, height: term.size.height };
  console.log(`shot ${name}`);
}

/** The screen's lines, as text. */
async function screen() {
  return (await term.text()).split("\n");
}

/** Waits until `test` finds something on the screen's lines; what it found. */
async function seek(what, test) {
  for (let tries = 0; tries < 150; tries++) {
    const found = test(await screen());
    if (found) return found;
    await sleep(100);
  }
  throw new Error(`never found ${what} on the screen:\n${(await screen()).join("\n")}`);
}

/** Where the list's frame ends: its top-right corner on the second line. */
async function listEdge() {
  const second = (await screen())[1] ?? "";
  return second.indexOf("┐");
}

/** The list's line of a session (its name and, under it, what it does), searched in the list only. */
async function row(title, rows = 2) {
  const edge = await listEdge();
  const found = await seek(title, (lines) => {
    for (let at = 1; at < lines.length; at++) {
      const column = lines[at].indexOf(title);
      if (column >= 0 && column < edge) return at;
    }
    return null;
  });
  return term.box(found, 1, rows, edge - 1);
}

/**
 * The frame (a dialog or a panel, drawn in box lines) titled `title`: the
 * title right after its top-left corner, to its bottom-right one.
 */
async function frame(title) {
  const escaped = title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`[┌╭┏]─?\\s*${escaped}`);
  return seek(`the frame ${title}`, (lines) => {
    for (let at = 0; at < lines.length; at++) {
      const match = pattern.exec(lines[at]);
      if (!match) continue;
      const left = match.index;
      const right = lines[at].slice(left).search(/[┐╮┓]/) + left;
      let bottom = at + 1;
      while (bottom < lines.length && !"└╰┗".includes(lines[bottom][left])) bottom++;
      if (right <= left || bottom >= lines.length) continue;
      return term.box(at, left, bottom - at + 1, right - left + 1);
    }
    return null;
  });
}

/** The `count` lines from the one holding `text`, `columns` wide from where it starts. */
async function lines(text, count, columns) {
  const found = await term.waitFor(text);
  return term.box(found.row, found.column, count, columns);
}

/** A run of text on the screen, and `extra` columns after it. */
async function text(what, extra = 0) {
  const found = await term.waitFor(what);
  return term.box(found.row, found.column, 1, found.length + extra);
}

try {
  const muster = world.repository("muster", {
    "README.md": "# muster\n",
    "Cargo.toml": '[workspace]\nmembers = ["crates/*"]\n',
    "src/view.rs": "pub fn view() {\n    rows();\n    header();\n}\n",
    "src/settings.rs": "pub struct Settings;\n",
  });
  const lamphaus = world.repository("lamphaus", {
    "README.md": "# lamphaus\n",
    "app/build.gradle": "plugins { id 'com.android.application' version '8.7.0' }\n",
  });
  writeFileSync(
    join(world.gh, "issues-muster.json"),
    JSON.stringify([
      { number: 214, title: "Desktop app: sessions list and one live terminal", url: "https://github.com/you/muster/issues/214", labels: [{ name: "desktop" }], author: { login: "you" } },
      { number: 205, title: "Resize settles once on release", url: "https://github.com/you/muster/issues/205", labels: [{ name: "bug" }], author: { login: "mira-k" } },
      { number: 198, title: "Light mode for the notch", url: "https://github.com/you/muster/issues/198", labels: [], author: { login: "sam" } },
    ]),
  );
  writeFileSync(
    join(world.gh, "issue-214.json"),
    JSON.stringify({
      number: 214,
      title: "Desktop app: sessions list and one live terminal",
      url: "https://github.com/you/muster/issues/214",
      labels: [{ name: "desktop" }],
      author: { login: "you" },
      body: "The first slice of the desktop app: a window that lists sessions and shows one live terminal, as another client of the daemon.",
      comments: [],
    }),
  );
  await world.startDaemon();
  world.muster(["project", "add", muster]);
  world.muster(["project", "add", lamphaus]);

  // A working day under way: agents at work, one finished.
  const agents = [
    { repo: muster, title: "Split view.rs by responsibility", tool: { tool_name: "Edit", tool_input: { file_path: "src/view.rs" } }, read: "src/view.rs" },
    { repo: muster, title: "Fix the flaky resize test", read: "crates/muster-tui/src/resize.rs", done: true },
    { repo: lamphaus, title: "Upgrade AGP to 9.1", tool: { tool_name: "Bash", tool_input: { command: "./gradlew assembleDebug" } }, read: "app/build.gradle" },
  ];
  for (const agent of agents) {
    world.muster(["claude", agent.title], agent.repo);
    const { id } = world.session(agent.title);
    await world.started(id);
    await world.hook(id, { hook_event_name: "SessionStart", session_id: id, source: "startup" });
    world.show(id, say([`${ORANGE}✻${RESET} ${agent.title}`, "", `${GREEN}●${RESET} Read(${agent.read})`, `  ⎿  Read 212 lines`, ""]));
    await world.hook(id, { hook_event_name: "UserPromptSubmit", session_id: id });
    if (agent.tool) await world.hook(id, { hook_event_name: "PreToolUse", session_id: id, ...agent.tool });
    if (agent.done) {
      world.show(id, say([`${GREEN}●${RESET} The resize settles once the button is released; the test waits for it.`]));
      await world.turn(id, { "crates/muster-tui/tests/resize.rs": "#[test]\nfn settles_once() {}\n" });
    }
  }
  const split = world.session("Split view.rs").id;

  term = await openTerminal(world, muster, { width: 1440, height: 900 });
  await term.waitFor("Split view.rs");
  await shot("home");
  // 1. Every agent in one list: the selected one's terminal beside it.
  await term.press("ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown");
  await term.waitFor("Split view.rs by responsibility ·");
  await shot("list", {
    "row-split": row("Split view.rs"),
    "row-flaky": row("Fix the flaky"),
    "row-agp": row("Upgrade AGP"),
    "list-frame": frame("projects"),
    preview: frame("Split view.rs by responsibility ·"),
    counts: text("2 working", 10),
  });

  // 2. A worktree of its own, and an agent started in it from a prompt.
  await term.press("ArrowUp", "ArrowUp", "ArrowUp");
  await term.press("w");
  await sleep(600);
  await shot("worktree", { "worktree-dialog": frame("New worktree") });
  await term.type("dark-mode");
  await shot("worktree-typed", { "worktree-dialog": frame("New worktree"), "worktree-text": text("dark-mode", 1) });
  await term.press("Enter");
  await term.waitFor("dark-mode");
  await shot("worktree-made", { "worktree-line": row("dark-mode", 1) });
  await term.press("n");
  await term.waitFor("New session");
  await shot("new-session", { "session-dialog": frame("New session") });
  await term.type("Add a dark mode to Settings");
  await shot("new-typed", { "session-dialog": frame("New session"), "session-text": text("Add a dark mode to Settings", 1) });
  await term.press("Enter");
  await term.waitFor("Add a dark mode");
  const added = world.session("Add a dark mode").id;
  await world.started(added);
  await world.hook(added, { hook_event_name: "SessionStart", session_id: added, source: "startup" });
  world.show(added, say([`${ORANGE}✻${RESET} Add a dark mode to Settings`, "", `${GREEN}●${RESET} Read(src/settings.rs)`, "  ⎿  Read 1 line", ""]));
  await world.hook(added, { hook_event_name: "UserPromptSubmit", session_id: added });
  await world.hook(added, { hook_event_name: "PreToolUse", session_id: added, tool_name: "Read", tool_input: { file_path: "src/settings.rs" } });
  await sleep(800);
  await shot("started", { "row-new": row("Add a dark mode"), "new-preview": frame("Add a dark mode to Settings ·") });
  // Starting it gave it the keys; back to the list.
  await term.press("Shift+Q");
  await term.waitGone("keys go to the session");

  // 3. One needs you: it asks, the queue has it, the answer goes to it.
  world.show(split, say([`${GREEN}●${RESET} Moved the rows to src/rows.rs and the header to src/header.rs.`, "", `  ${YELLOW}Allow Claude to run this command?${RESET}`, "", "  $ cargo build", "", `  ${ORANGE}❯ 1. Yes${RESET}`, "    2. No, and tell Claude what to do differently", ""]));
  await world.hook(split, { hook_event_name: "PermissionRequest", session_id: split, tool_name: "Bash", tool_input: { command: "cargo build" } });
  await term.waitFor("waiting");
  await shot("asking", { "row-asking": row("Split view.rs"), waiting: text("1 waiting"), "needs-you-tab": text("Needs you 2") });
  await term.press("Tab");
  await term.waitFor("waiting on you");
  await shot("queue", { queue: frame("waiting on you"), question: lines("Allow Claude to run this command?", 7, 52) });
  await term.press("Enter");
  await term.waitFor("keys go to the session");
  await shot("answering", { question: lines("Allow Claude to run this command?", 7, 52) });
  await term.press("1");
  world.show(split, say([`${GREEN}✔${RESET} You allowed: cargo build`, `${DIM}   Compiling muster v0.7.4${RESET}`, `${DIM}    Finished \`dev\` profile in 9.81s${RESET}`, "", `${GREEN}●${RESET} Built. view.rs now only lays the screen out.`]));
  await world.hook(split, { hook_event_name: "PostToolUse", session_id: split, tool_name: "Bash", tool_input: { command: "cargo build" } });
  // Its turn's work, in its worktree, then the turn's end.
  await world.turn(split, {
    "src/view.rs": "mod header;\nmod rows;\n\npub fn view() {\n    rows::rows();\n    header::header();\n}\n",
    "src/rows.rs": "pub fn rows() {\n    // Each session's line.\n}\n",
    "src/header.rs": "pub fn header() {\n    // The counts and the tabs.\n}\n",
  }, { end: false });
  await sleep(500);
  await world.hook(split, { hook_event_name: "Stop", session_id: split });
  await term.waitFor("finished");
  await sleep(2500);
  await shot("answered", { allowed: lines("You allowed: cargo build", 5, 52), "row-answered": row("Split view.rs") });
  await term.press("Shift+Q");
  await term.waitGone("keys go to the session");

  // 4. What its turn changed, and the turn put back.
  await term.press("d");
  await term.waitFor("Claude Code · turn 1");
  await shot("changes", { changes: frame("Changes ·"), turn: lines("#1  Claude Code · turn 1", 1, 44) });
  await term.press("Enter");
  await sleep(600);
  await shot("turn-diff", { diff: frame("#1 · Claude Code · turn 1") });
  await term.press("Escape");
  await sleep(400);
  await term.press("z");
  await sleep(700);
  await shot("put-back", { "put-back-dialog": frame("Put the files back?") });
  await term.press("y");
  await sleep(1500);
  await shot("put-back-done", { "changes-after": frame("Changes ·"), "put-back-line": lines("#2  Put back as before #1", 1, 44) });

  // 5. A ticket to an agent.
  await term.press("d");
  await sleep(400);
  await term.press("i");
  await term.waitFor("#214");
  // Onto the ticket: its text shows beside the list.
  await term.press("ArrowDown");
  await term.waitFor("No agent on it yet");
  await shot("tickets", { "ticket-list": frame("tickets"), ticket: frame("Ticket"), "ticket-line": lines("#214 Desktop app", 1, 30) });
  await term.press("Enter");
  await term.waitFor("Assign an agent");
  await shot("assign", { "assign-dialog": frame("Assign an agent") });
  await term.press("Enter");
  await term.waitFor("#214 Desktop app: sessions list and one live terminal ·");
  const ticketed = world.session("#214").id;
  await world.started(ticketed);
  await world.hook(ticketed, { hook_event_name: "SessionStart", session_id: ticketed, source: "startup" });
  world.show(ticketed, say([`${ORANGE}✻${RESET} #214 Desktop app: sessions list and one live terminal`, "", `${GREEN}●${RESET} Read(src/app/App.tsx)`, "  ⎿  Read 188 lines", "", `${GREEN}●${RESET} The list comes from the daemon's snapshot; I'll stream the selected`, "  session's terminal beside it.", ""]));
  await world.hook(ticketed, { hook_event_name: "UserPromptSubmit", session_id: ticketed });
  await sleep(1500);
  await shot("assigned", { "ticket-agent": frame("#214 Desktop app: sessions list and one live terminal ·"), "ticket-mark": row("#214 Desktop app", 1) });
} catch (error) {
  console.error(error.message);
  if (term) {
    await term.screenshot(join(out, "failed.png"));
    writeFileSync(join(out, "failed.txt"), await term.text());
  }
  process.exitCode = 1;
} finally {
  if (term) await term.close();
  world.stop();
}

const source = [
  "// Written by record.mjs: the terminal UI's shots, and every crop on them,",
  "// measured from the terminal's grid, in its pixels (the PNGs are twice that).",
  "export const SCALE = 2;",
  `export const shots = ${JSON.stringify(shots, null, 2)};`,
  `export const crops = ${JSON.stringify(crops, null, 2)};`,
  "export const points = {};",
  "",
].join("\n");
writeFileSync(join(here, "crops.js"), source);
