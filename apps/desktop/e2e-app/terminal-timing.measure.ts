import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { $, browser } from "@wdio/globals";
import { startAgent } from "./support/agents";
import { APP } from "./support/harness";
import { muster, session, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { bareId, goTo, row, selectRow, terminalText, typeInTerminal } from "./support/window";

// Measures, never asserts: how long a key takes to come back on screen in
// a session's terminal, how an agent's streaming redraws reach it (each
// frame stamped with the time its program wrote it, read back off the
// screen as xterm draws it), the app's CPU meanwhile, and how long the
// terminal takes to catch up once the window is seen again. Not one of the
// specs `pnpm e2e:app` runs: build the app as it does (a release build for
// numbers worth keeping), then
//   pnpm exec wdio run wdio.conf.ts --spec e2e-app/terminal-timing.measure.ts
// PROBE_FPS, PROBE_LINES, PROBE_SECONDS, PROBE_WIDTH and PROBE_HEIGHT set
// the load; MUSTER_E2E_MUSTER a release `muster`.

const TITLE = "Probe";
const FPS = Number(process.env.PROBE_FPS ?? 30);
const SECONDS = Number(process.env.PROBE_SECONDS ?? 6);
const LINES = Number(process.env.PROBE_LINES ?? 14);
const WIDTH = Number(process.env.PROBE_WIDTH ?? 1500);
const HEIGHT = Number(process.env.PROBE_HEIGHT ?? 1000);

/** CPU % of the app and of the busiest WebKit content process, sampled by ps. */
function cpu(): { app: number; web: number } {
  const lines = execFileSync("ps", ["-Ao", "%cpu=,command="], { encoding: "utf8" }).split("\n");
  let app = 0;
  let web = 0;
  for (const line of lines) {
    const [, share, command] = /^\s*([\d.]+)\s+(.*)$/.exec(line) ?? [];
    const value = Number(share ?? 0);
    if (command?.startsWith(APP)) app += value;
    if (command?.includes("WebKit.WebContent")) web = Math.max(web, value);
  }
  return { app, web };
}

/** Draws agent-like frames: a region of lines redrawn in place, in colour, synchronized. */
const GENERATOR = `use Time::HiRes qw(time sleep);
my ($fps, $secs, $lines, $label) = @ARGV;
$| = 1;
my ($frame, $start) = (0, time);
my $next = $start;
print "\\n" x $lines;
while (time - $start < $secs) {
  my $out = "\\e[?2026h\\e[\${lines}A\\r";
  for my $i (1 .. $lines - 1) {
    $out .= "\\e[2K\\e[38;2;200;120;50m*\\e[m line $i of the answer, streaming " . ("tok " x (($frame + $i) % 18)) . "\\r\\n";
  }
  $out .= sprintf("\\e[2K\\e[1mframe %d t=%.3f\\e[m\\r\\n", $frame, time * 1000);
  $out .= "\\e[?2026l";
  print $out;
  $frame++;
  $next += 1 / $fps;
  my $wait = $next - time;
  sleep($wait) if $wait > 0;
}
print "streamed $frame frames $label\\r\\n";
`;

function stats(values: number[]): string {
  if (values.length === 0) return "none";
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
  const mean = values.reduce((sum, value) => sum + value, 0) / values.length;
  return `n=${values.length} mean=${mean.toFixed(1)} p50=${at(0.5).toFixed(1)} p90=${at(0.9).toFixed(1)} p99=${at(0.99).toFixed(1)} max=${(sorted.at(-1) ?? 0).toFixed(1)}`;
}

describe("A session's terminal, timed", () => {
  let id = "";
  let repo = "";

  before(async () => {
    await browser.setWindowRect(40, 40, WIDTH, HEIGHT);
    await until(() => status().running, "the app to start its daemon", 30_000);
    repo = repository("app");
    muster("project", "add", repo);
    await startAgent("app", TITLE);
    id = bareId(session(TITLE)?.id ?? "");
    await goTo("Sessions");
    await row(TITLE).waitForExist({ timeout: 20_000 });
    await selectRow(TITLE);
    await until(async () => (await terminalText(id)).includes("$"), "the stand-in's prompt");
    writeFileSync(join(repo, "frames.pl"), GENERATOR);
  });

  it("draws on the GPU", async () => {
    const drawn = await browser.execute(() => {
      const term = document.querySelector('section[aria-label="Terminal"]');
      return {
        canvases: term?.querySelectorAll("canvas").length ?? 0,
        domRows: term?.querySelectorAll(".xterm-rows > div").length ?? 0,
        webgl2: Boolean(document.createElement("canvas").getContext("webgl2")),
        ratio: window.devicePixelRatio,
      };
    });
    console.log(`RENDERER ${JSON.stringify(drawn)}`);
  });

  it("echoes keys", async () => {
    await $('section[aria-label="Terminal"]').click();
    const times = (await browser.executeAsync(
      (session: string, done: (times: number[]) => void) => {
        const x = (
          window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
        ).__musterXterms[session];
        if (!x) throw new Error("no terminal exposed: an app built for the end-to-end tests?");
        const times: number[] = [];
        const line = () => {
          const buffer = x.buffer.active;
          return buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(true) ?? "";
        };
        let round = 0;
        const next = () => {
          if (round === 40) {
            x.input("\u0015", true);
            done(times);
            return;
          }
          round += 1;
          const before = line().length;
          const start = performance.now();
          const seen = x.onRender(() => {
            if (line().length > before) {
              seen.dispose();
              times.push(performance.now() - start);
              setTimeout(next, 30 + Math.random() * 40);
            }
          });
          x.input("a", true);
        };
        next();
      },
      id,
    )) as number[];
    console.log(`ECHO key to screen (ms): ${stats(times)}`);
  });

  it("draws an agent's streaming redraws", async () => {
    await browser.execute((session: string) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[session];
      if (!x) throw new Error("no terminal exposed: an app built for the end-to-end tests?");
      const probe = {
        renders: [] as number[],
        shown: [] as [number, number, number][],
        raf: [] as number[],
        stop: false,
      };
      (window as unknown as { __probe: typeof probe }).__probe = probe;
      x.onRender(() => {
        const now = performance.timeOrigin + performance.now();
        probe.renders.push(now);
        const buffer = x.buffer.active;
        for (let row = buffer.cursorY; row >= Math.max(0, buffer.cursorY - 3); row -= 1) {
          const text = buffer.getLine(buffer.baseY + row)?.translateToString(true) ?? "";
          const found = /frame (\d+) t=([\d.]+)/.exec(text);
          if (found) {
            probe.shown.push([Number(found[1]), Number(found[2]), now]);
            break;
          }
        }
      });
      const loop = (at: number) => {
        probe.raf.push(at);
        if (!probe.stop) requestAnimationFrame(loop);
      };
      requestAnimationFrame(loop);
    }, id);
    await typeInTerminal(`perl frames.pl ${FPS} ${SECONDS} ${LINES}`);
    // Not looked at meanwhile: reading the whole terminal costs the page.
    const samples: { app: number; web: number }[] = [];
    for (let at = 0; at < SECONDS; at += 1) {
      await new Promise((done) => setTimeout(done, 1000));
      samples.push(cpu());
    }
    await browser.pause(1000);
    await until(async () => (await terminalText(id)).includes("streamed "), "the frames", 20_000);
    const probe = (await browser.execute(() => {
      const probe = (window as unknown as { __probe: { stop: boolean } }).__probe;
      probe.stop = true;
      return probe;
    })) as unknown as { renders: number[]; shown: [number, number, number][]; raf: number[] };
    const shown = probe.shown.filter(([frame], at, all) => at === 0 || frame !== all[at - 1]?.[0]);
    const latency = shown.map(([, wrote, drawn]) => drawn - wrote);
    const frames = shown.map(([frame]) => frame);
    const skipped = frames.slice(1).map((frame, at) => frame - (frames[at] ?? frame) - 1);
    const gaps = shown.slice(1).map(([, , drawn], at) => drawn - (shown[at]?.[2] ?? drawn));
    const raf = probe.raf.slice(1).map((at, n) => at - (probe.raf[n] ?? at));
    const written = FPS * SECONDS;
    console.log(
      `STREAM ${FPS} fps x ${SECONDS} s, ${LINES} lines: ${written} frames written, ${shown.length} drawn`,
    );
    console.log(`STREAM write to screen (ms): ${stats(latency)}`);
    console.log(`STREAM frames skipped between draws: ${stats(skipped)}`);
    console.log(`STREAM time between frames drawn (ms): ${stats(gaps)}`);
    console.log(`STREAM page frame intervals (ms): ${stats(raf)}`);
    console.log(`STREAM renders: ${probe.renders.length}`);
    console.log(`STREAM cpu % app: ${stats(samples.map((one) => one.app))}`);
    console.log(`STREAM cpu % web: ${stats(samples.map((one) => one.web))}`);
    const size = await browser.execute((session: string) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[session];
      return x ? `${x.cols}x${x.rows}` : "?";
    }, id);
    console.log(`STREAM terminal ${size}`);
  });

  it("catches up once the window is seen again", async () => {
    // Out of sight, as the app is told on ⌘Tab away: its terminals let
    // go, and the program streams on.
    const seen = (visible: boolean) =>
      browser.execute(
        (state: string) => {
          Object.defineProperty(document, "visibilityState", { value: state, configurable: true });
          document.dispatchEvent(new Event("visibilitychange"));
        },
        visible ? "visible" : "hidden",
      );
    await seen(false);
    const typed = await browser.execute(
      (session: string, line: string) => {
        const x = (
          window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
        ).__musterXterms[session];
        x?.input(`${line}\r`, true);
        return document.visibilityState;
      },
      id,
      `perl frames.pl ${FPS} ${SECONDS} ${LINES} unseen`,
    );
    console.log(`CATCHUP typed while ${typed}`);
    await new Promise((done) => setTimeout(done, (SECONDS + 1) * 1000));
    await browser.execute(() => {
      const probe = { raf: [] as number[], seen: 0, stop: false };
      (window as unknown as { __catchup: typeof probe }).__catchup = probe;
      document.addEventListener(
        "visibilitychange",
        () => {
          probe.seen = performance.now();
        },
        { once: true },
      );
      const loop = (at: number) => {
        probe.raf.push(at);
        if (!probe.stop) requestAnimationFrame(loop);
      };
      requestAnimationFrame(loop);
    });
    await seen(true);
    const started = Date.now();
    await until(
      async () => / frames unseen/.test(await terminalText(id)),
      "the second stream's end",
      30_000,
    );
    const took = Date.now() - started;
    const probe = (await browser.execute(() => {
      const probe = (window as unknown as { __catchup: { stop: boolean } }).__catchup;
      probe.stop = true;
      return probe;
    })) as unknown as { raf: number[]; seen: number };
    const after = probe.raf.filter((at) => at >= probe.seen);
    const raf = after.slice(1).map((at, n) => at - (after[n] ?? at));
    console.log(`CATCHUP shown to last frame (ms, polled): ${took}`);
    console.log(`CATCHUP page frame intervals after shown (ms): ${stats(raf)}`);
    const shownNow = await browser.execute((session: string) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[session];
      const buffer = x?.buffer.active;
      if (!buffer) return [];
      const lines: string[] = [];
      for (let row = 0; row < buffer.length; row += 1)
        lines.push(buffer.getLine(row)?.translateToString(true) ?? "");
      return lines.slice(-6);
    }, id);
    console.log(`CATCHUP screen ends with ${JSON.stringify(shownNow)}`);
  });
});
