import { readFileSync, writeFileSync } from "node:fs";
import { $, browser } from "@wdio/globals";
import { startAgent } from "./support/agents";
import { muster, session, status, until } from "./support/muster";
import { repository } from "./support/repos";
import {
  bareId,
  goTo,
  pressOnWindow,
  row,
  selectRow,
  shownScreen,
  terminalText,
} from "./support/window";

// Measures, never asserts: what it costs a session's terminal to come back
// on screen in the real app, after switching sessions, opening or putting
// away the shell, the changes, or Tickets, and how many xterms it was given
// (`__musterXtermsMade`, src/terminal/testing.ts). Each session's terminal
// holds a recorded agent's output, written to its program's terminal so it
// takes the daemon's ring and the link like any output; while it is out of
// sight a new marker is written, and coming back is timed until that marker
// is drawn, not the text it had. Not one of the specs `pnpm e2e:app` runs:
// build the app as it does, then
//   PROBE_RECORDINGS=a.bin,b.bin pnpm exec wdio run wdio.conf.ts --spec e2e-app/terminal-lifecycle.measure.ts
// A recording is a program's output with each read stamped `\n@@<seconds>@@`
// before it (the stamps and the program's questions to its terminal are
// taken out); PROBE_FILL bytes (1 MiB, the daemon's ring) go to each session.

const RECORDINGS = (process.env.PROBE_RECORDINGS ?? "").split(",").filter(Boolean);
const FILL = Number(process.env.PROBE_FILL ?? 1024 * 1024);
const ROUNDS = Number(process.env.PROBE_ROUNDS ?? 5);
const WIDTH = Number(process.env.PROBE_WIDTH ?? 1500);
const HEIGHT = Number(process.env.PROBE_HEIGHT ?? 1000);
const SESSIONS = ["Alpha", "Beta"] as const;

/** The recordings' output, without stamps or questions to the terminal, to `size` bytes. */
function recorded(size: number): Buffer {
  const parts = RECORDINGS.map((path) =>
    Buffer.from(
      readFileSync(path)
        .toString("latin1")
        .replace(/\n@@[0-9.]+@@/g, "")
        // Questions a program asks its terminal, which the daemon would answer.
        // biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences are what it takes out.
        .replace(/\x1b\[[>?]?[0-9;]*[cnqu]|\x1b\][0-9]+;\?(\x07|\x1b\\)/g, ""),
      "latin1",
    ),
  );
  const all = Buffer.concat(parts);
  // Cut anywhere, a sequence may be left open: it is ended (ST, CAN), its
  // colours let go, and the mouse given back, so the shell under it is
  // typed no reports by the tests' clicks. Then the scroll region, the
  // alternate screen and the screen itself are put back as they began, so
  // the daemon's screen and xterm agree on what follows, whatever of the
  // recording each took differently.
  const end = Buffer.from(
    "\x1b\\\x18\x1b[0m\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[r\x1b[?1049l\x1b[H\x1b[2J",
    "latin1",
  );
  const out = Buffer.alloc(size);
  const body = size - end.length;
  for (let at = 0; at < body; at += all.length)
    all.copy(out, at, 0, Math.min(all.length, body - at));
  end.copy(out, body);
  return out;
}

function stats(all: number[]): string {
  const missed = all.filter((value) => Number.isNaN(value)).length;
  const values = all.filter((value) => !Number.isNaN(value));
  if (missed > 0) return `${stats(values)} (${missed} not drawn in 30 s)`;
  if (values.length === 0) return "none";
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  return `n=${values.length} mean=${mean.toFixed(1)} p50=${at(0.5).toFixed(1)} p90=${at(0.9).toFixed(1)} max=${(sorted.at(-1) ?? 0).toFixed(1)}`;
}

const ttys: Record<string, string> = {};
const ids: Record<string, string> = {};
let marks = 0;

/** Writes a new marker to `title`'s program's terminal: what coming back must show. */
function mark(title: string, before: Buffer | null = null): string {
  marks += 1;
  const marker = `MARK-${title}-${marks}`;
  const tty = ttys[title];
  if (!tty) throw new Error(`no terminal for ${title}`);
  writeFileSync(tty, Buffer.concat([before ?? Buffer.alloc(0), Buffer.from(`\r\n${marker}\r\n`)]));
  return marker;
}

/** How many xterms each session's terminal has been given so far. */
function made(): Promise<Record<string, number>> {
  return browser.execute(() => ({
    ...(window as unknown as { __musterXtermsMade?: Record<string, number> }).__musterXtermsMade,
  }));
}

/**
 * Times `act` from the first key or press it makes in the page until
 * `title`'s terminal draws `marker` on its screen, and counts the xterms
 * each session was given meanwhile.
 */
async function timed(
  title: string,
  marker: string,
  act: () => Promise<void>,
): Promise<{ ms: number; made: number }> {
  const id = ids[title] ?? "";
  const before = await made();
  await browser.execute(
    (session: string, wanted: string) => {
      const probe = { start: 0, ms: -1 };
      const holder = window as unknown as { __shown?: typeof probe };
      // Only the newest probe looks: one that timed out stops.
      holder.__shown = probe;
      const begin = () => {
        if (probe.start === 0) probe.start = performance.now();
      };
      // The embedded driver's click is a click alone, with no press before it.
      const kinds = ["pointerdown", "mousedown", "click", "keydown"];
      for (const kind of kinds) window.addEventListener(kind, begin, { capture: true });
      const stop = () => {
        for (const kind of kinds) window.removeEventListener(kind, begin, { capture: true });
      };
      const look = () => {
        if (holder.__shown !== probe) return stop();
        const x = (
          window as unknown as { __musterXterms?: Record<string, import("@xterm/xterm").Terminal> }
        ).__musterXterms?.[session];
        const buffer = x?.buffer.active;
        let found = false;
        if (buffer)
          for (let at = buffer.viewportY; at < buffer.viewportY + (x?.rows ?? 0) && !found; at += 1)
            found = buffer.getLine(at)?.translateToString(true).includes(wanted) ?? false;
        if (found && probe.start > 0) {
          probe.ms = performance.now() - probe.start;
          return stop();
        }
        requestAnimationFrame(look);
      };
      requestAnimationFrame(look);
    },
    id,
    marker,
  );
  await act();
  let ms: number;
  try {
    ms = await until(
      () =>
        browser.execute(() => {
          const probe = (window as unknown as { __shown: { ms: number } }).__shown;
          return probe.ms >= 0 ? { ms: probe.ms } : null;
        }),
      `${title}'s terminal to draw ${marker}`,
      30_000,
    ).then((found) => found.ms);
  } catch (error) {
    const seen = await browser.execute(
      (session: string, wanted: string) => {
        const x = (
          window as unknown as { __musterXterms?: Record<string, import("@xterm/xterm").Terminal> }
        ).__musterXterms?.[session];
        const buffer = x?.buffer.active;
        if (!x || !buffer) return { exposed: false };
        const rows: string[] = [];
        for (let at = buffer.viewportY; at < buffer.viewportY + x.rows; at += 1)
          rows.push(buffer.getLine(at)?.translateToString(true) ?? "");
        let anywhere = -1;
        for (let at = 0; at < buffer.length && anywhere < 0; at += 1)
          if (buffer.getLine(at)?.translateToString(true).includes(wanted)) anywhere = at;
        const probe = (window as unknown as { __shown: { start: number } }).__shown;
        return {
          exposed: true,
          type: buffer.type,
          viewportY: buffer.viewportY,
          baseY: buffer.baseY,
          length: buffer.length,
          anywhere,
          started: probe.start > 0,
          rows: rows.filter(Boolean).slice(-6),
        };
      },
      id,
      marker,
    );
    console.log(`LIFECYCLE ${title} missing ${marker}: ${JSON.stringify(seen)} (${error})`);
    // Not drawn in time: counted as such, and the xterms made still told.
    ms = Number.NaN;
  }
  const after = await made();
  return { ms, made: (after[id] ?? 0) - (before[id] ?? 0) };
}

function report(label: string, runs: { ms: number; made: number }[]): void {
  console.log(
    `LIFECYCLE ${label}: back on screen (ms) ${stats(runs.map((run) => run.ms))}; xterms made ${runs.map((run) => run.made).join(",")}`,
  );
}

describe("A session's terminal coming back on screen", () => {
  before(async () => {
    if (RECORDINGS.length === 0) throw new Error("PROBE_RECORDINGS names no recording");
    await browser.setWindowRect(40, 40, WIDTH, HEIGHT);
    await until(() => status().running, "the app to start its daemon", 30_000);
    muster("project", "add", repository("app"));
    const fill = recorded(FILL);
    for (const title of SESSIONS) {
      const launch = await startAgent("app", title);
      ttys[title] = launch.tty;
      ids[title] = bareId(session(title)?.id ?? "");
      writeFileSync(launch.tty, fill);
    }
    await goTo("Sessions");
    for (const title of SESSIONS) await row(title).waitForExist({ timeout: 20_000 });
    // Both drawn once, as a person who has looked at each.
    for (const title of [...SESSIONS, "Alpha"]) {
      const marker = mark(title);
      await selectRow(title);
      try {
        await until(
          async () => (await terminalText(ids[title] ?? "")).includes(marker),
          `${title}'s output`,
          60_000,
        );
      } catch (error) {
        const text = await terminalText(ids[title] ?? "");
        console.log(
          `LIFECYCLE ${title} shows ${text.length} chars, ending ${JSON.stringify(text.slice(-400))}`,
        );
        throw error;
      }
    }
  });

  it("switching sessions", async () => {
    const runs: { ms: number; made: number }[] = [];
    for (let round = 0; round < ROUNDS; round += 1) {
      for (const title of ["Beta", "Alpha"] as const) {
        const marker = mark(title);
        runs.push(await timed(title, marker, () => row(title).click()));
      }
    }
    report("switch sessions", runs);
  });

  it("switching to a session whose output overran the ring while away", async () => {
    const runs: { ms: number; made: number }[] = [];
    for (let round = 0; round < 2; round += 1) {
      await row("Alpha").click();
      const marker = mark("Beta", recorded(FILL + 256 * 1024));
      runs.push(await timed("Beta", marker, () => row("Beta").click()));
    }
    report("switch after ring overrun", runs);
  });

  it("opening the shell beside the agent and putting it away", async () => {
    await row("Alpha").click();
    const open: { ms: number; made: number }[] = [];
    const away: { ms: number; made: number }[] = [];
    for (let round = 0; round < ROUNDS; round += 1) {
      open.push(await timed("Alpha", mark("Alpha"), () => pressOnWindow("T")));
      await $('section.mu-panel[aria-label="Shell"]').waitForDisplayed({ timeout: 20_000 });
      away.push(
        await timed("Alpha", mark("Alpha"), () =>
          $('button[aria-label="Put the shell away"]').click(),
        ),
      );
    }
    report("shell opened (agent's terminal)", open);
    report("shell put away (agent's terminal)", away);
  });

  it("the changes and back", async () => {
    const runs: { ms: number; made: number }[] = [];
    for (let round = 0; round < ROUNDS; round += 1) {
      await $('button[aria-label="Changes"]').click();
      await browser.pause(300);
      const marker = mark("Alpha");
      runs.push(await timed("Alpha", marker, () => $('button[aria-label="Changes"]').click()));
    }
    report("changes and back", runs);
  });

  it("Tickets and back", async () => {
    const runs: { ms: number; made: number }[] = [];
    for (let round = 0; round < ROUNDS; round += 1) {
      await goTo("Tickets");
      const marker = mark("Alpha");
      // The Sessions tab's key, as the person goes back (its sidebar button is
      // pressed already, so a click on it shows nothing new).
      runs.push(await timed("Alpha", marker, () => pressOnWindow("1")));
      await browser.waitUntil(async () => (await shownScreen()) === "Sessions", {
        timeoutMsg: "Sessions was not shown",
      });
    }
    report("Tickets and back", runs);
  });

  it("echoes keys over a full ring", async () => {
    await row("Alpha").click();
    await $('section[aria-label="Terminal"]').click();
    const times = (await browser.executeAsync((id: string, done: (times: number[]) => void) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[id];
      if (!x) throw new Error("no terminal exposed");
      const times: number[] = [];
      const line = () => {
        const buffer = x.buffer.active;
        return buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(true) ?? "";
      };
      let round = 0;
      const next = () => {
        if (round === 25) {
          x.input("\u0015", true);
          done(times);
          return;
        }
        round += 1;
        const before = line();
        const start = performance.now();
        const seen = x.onRender(() => {
          if (line() !== before) {
            seen.dispose();
            times.push(performance.now() - start);
            requestAnimationFrame(() => requestAnimationFrame(next));
          }
        });
        x.input("a", true);
      };
      next();
    }, ids.Alpha ?? "")) as number[];
    console.log(`LIFECYCLE echo key to screen (ms): ${stats(times)}`);
  });
});
