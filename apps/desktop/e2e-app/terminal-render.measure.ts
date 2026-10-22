import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { browser } from "@wdio/globals";
import { startAgent } from "./support/agents";
import { APP } from "./support/harness";
import { muster, session, status, until } from "./support/muster";
import { repository } from "./support/repos";
import { bareId, goTo, row, selectRow, terminalText } from "./support/window";

// Measures, never asserts: what drawing a recorded agent's output costs a
// session's terminal in the real app, on the GPU (xterm's WebGL renderer)
// and with the DOM renderer it falls back to. The recording is played into
// the terminal itself at the pace it was written, past the daemon, so only
// drawing is timed: the glyph atlas's uploads (`texImage2D`) and readbacks
// (`getImageData`), the page's frame intervals, and the app's CPU. Not one
// of the specs `pnpm e2e:app` runs: build the app as it does, then
//   PROBE_RECORDINGS=a.bin,b.bin pnpm exec wdio run wdio.conf.ts --spec e2e-app/terminal-render.measure.ts
// A recording is a program's output with each read stamped `\n@@<seconds>@@`
// before it; pauses over PROBE_MAX_PAUSE seconds (1) are cut to it.

const TITLE = "Probe";
const RECORDINGS = (process.env.PROBE_RECORDINGS ?? "").split(",").filter(Boolean);
const MAX_PAUSE = Number(process.env.PROBE_MAX_PAUSE ?? 1);
const WIDTH = Number(process.env.PROBE_WIDTH ?? 1500);
const HEIGHT = Number(process.env.PROBE_HEIGHT ?? 1000);

/** Each read of the recordings, one after another: when to write it (ms from the start) and its bytes. */
function chunks(): { at: number; data: string }[] {
  const out: { at: number; data: string }[] = [];
  let at = 0;
  for (const path of RECORDINGS) {
    const parts = readFileSync(path)
      .toString("latin1")
      .split(/\n@@([0-9.]+)@@/);
    let last: number | null = null;
    for (let index = 1; index + 1 < parts.length; index += 2) {
      const stamp = Number(parts[index]);
      if (last !== null) at += Math.min(MAX_PAUSE, stamp - last) * 1000;
      last = stamp;
      out.push({ at, data: Buffer.from(parts[index + 1] ?? "", "latin1").toString("base64") });
    }
    at += 500;
  }
  return out;
}

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

function stats(values: number[]): string {
  if (values.length === 0) return "none";
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
  const sum = values.reduce((total, value) => total + value, 0);
  return `n=${values.length} sum=${sum.toFixed(0)} mean=${(sum / values.length).toFixed(1)} p50=${at(0.5).toFixed(1)} p90=${at(0.9).toFixed(1)} p99=${at(0.99).toFixed(1)} max=${(sorted.at(-1) ?? 0).toFixed(1)}`;
}

interface Played {
  uploads: number[];
  uploadPixels: number[];
  readbacks: number[];
  raf: number[];
  renders: number;
  canvases: number;
}

/** Plays the recordings into the terminal, timing what drawing them costs. */
async function play(id: string, label: string): Promise<void> {
  const recorded = chunks();
  const samples: { app: number; web: number }[] = [];
  const sampling = setInterval(() => samples.push(cpu()), 1000);
  await browser.execute(
    (session: string, recorded: { at: number; data: string }[]) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[session];
      if (!x) throw new Error("no terminal exposed: an app built for the end-to-end tests?");
      const played: Played = {
        uploads: [],
        uploadPixels: [],
        readbacks: [],
        raf: [],
        renders: 0,
        canvases: 0,
      };
      type Timed = (...args: unknown[]) => unknown;
      const gl = WebGL2RenderingContext.prototype as unknown as { texImage2D: Timed };
      const ctx = CanvasRenderingContext2D.prototype as unknown as { getImageData: Timed };
      const upload = gl.texImage2D;
      const readback = ctx.getImageData;
      gl.texImage2D = function (this: unknown, ...args: unknown[]) {
        const start = performance.now();
        const result = upload.apply(this, args);
        played.uploads.push(performance.now() - start);
        const source = args[args.length - 1] as { width?: number; height?: number } | null;
        played.uploadPixels.push((source?.width ?? 0) * (source?.height ?? 0));
        return result;
      };
      ctx.getImageData = function (this: unknown, ...args: unknown[]) {
        const start = performance.now();
        const result = readback.apply(this, args);
        played.readbacks.push(performance.now() - start);
        return result;
      };
      const rendered = x.onRender(() => {
        played.renders += 1;
      });
      let stop = false;
      const loop = (at: number) => {
        played.raf.push(at);
        if (!stop) requestAnimationFrame(loop);
      };
      requestAnimationFrame(loop);
      const bytes = (data: string) => Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
      const start = performance.now();
      let next = 0;
      const write = () => {
        while (next < recorded.length && (recorded[next]?.at ?? 0) <= performance.now() - start) {
          x.write(bytes(recorded[next]?.data ?? ""));
          next += 1;
        }
        if (next < recorded.length) {
          setTimeout(write, Math.max(0, (recorded[next]?.at ?? 0) - (performance.now() - start)));
          return;
        }
        x.write("", () =>
          setTimeout(() => {
            stop = true;
            rendered.dispose();
            gl.texImage2D = upload;
            ctx.getImageData = readback;
            played.canvases =
              document.querySelector('section[aria-label="Terminal"]')?.querySelectorAll("canvas")
                .length ?? 0;
            (window as unknown as { __played?: Played }).__played = played;
          }, 500),
        );
      };
      write();
    },
    id,
    recorded,
  );
  // Played at its own pace, longer than a script may wait: looked for until done.
  // Seldom, so looking costs the page nothing worth counting.
  await browser.waitUntil(
    () => browser.execute(() => Boolean((window as unknown as { __played?: unknown }).__played)),
    { timeout: 600_000, interval: 2000, timeoutMsg: "waited 10 min for the recording to play" },
  );
  const played = (await browser.execute(() => {
    const holder = window as unknown as { __played?: Played };
    const done = holder.__played;
    delete holder.__played;
    return done;
  })) as Played;
  clearInterval(sampling);
  const raf = played.raf.slice(1).map((at, n) => at - (played.raf[n] ?? at));
  console.log(`${label} canvases in the terminal: ${played.canvases}, renders: ${played.renders}`);
  console.log(`${label} texImage2D (ms): ${stats(played.uploads)}`);
  console.log(
    `${label} texImage2D megapixels: ${stats(played.uploadPixels.map((pixels) => pixels / 1e6))}`,
  );
  console.log(`${label} getImageData (ms): ${stats(played.readbacks)}`);
  console.log(`${label} page frame intervals (ms): ${stats(raf)}`);
  console.log(`${label} frames over 33 ms: ${raf.filter((gap) => gap > 33).length}`);
  console.log(`${label} cpu % app: ${stats(samples.map((one) => one.app))}`);
  console.log(`${label} cpu % web: ${stats(samples.map((one) => one.web))}`);
}

describe("A session's terminal drawing a recorded agent", () => {
  let id = "";

  before(async () => {
    if (RECORDINGS.length === 0) throw new Error("PROBE_RECORDINGS names no recording");
    await browser.setWindowRect(40, 40, WIDTH, HEIGHT);
    await until(() => status().running, "the app to start its daemon", 30_000);
    muster("project", "add", repository("app"));
    await startAgent("app", TITLE);
    id = bareId(session(TITLE)?.id ?? "");
    await goTo("Sessions");
    await row(TITLE).waitForExist({ timeout: 20_000 });
    await selectRow(TITLE);
    await until(async () => (await terminalText(id)).includes("$"), "the stand-in's prompt");
    const size = await browser.execute((session: string) => {
      const x = (
        window as unknown as { __musterXterms: Record<string, import("@xterm/xterm").Terminal> }
      ).__musterXterms[session];
      return x ? `${x.cols}x${x.rows}` : "?";
    }, id);
    console.log(
      `RENDER terminal ${size}, ratio ${await browser.execute(() => window.devicePixelRatio)}`,
    );
  });

  it("on the GPU, its atlas new", async () => {
    await play(id, "WEBGL cold");
  });

  it("on the GPU again, its atlas warm", async () => {
    await play(id, "WEBGL warm");
  });

  it("with the DOM renderer", async () => {
    // As after a sleep: the WebGL context lost, the terminal falls back to the DOM.
    await browser.execute(() => {
      for (const canvas of document.querySelectorAll('section[aria-label="Terminal"] canvas')) {
        (canvas as HTMLCanvasElement)
          .getContext("webgl2")
          ?.getExtension("WEBGL_lose_context")
          ?.loseContext();
      }
    });
    await browser.pause(500);
    await play(id, "DOM");
  });
});
