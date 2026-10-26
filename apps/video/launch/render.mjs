// Renders the clip from index.html's seek(t), in headless Chromium:
//
//   node launch/render.mjs --stills 7,17,46   one PNG per beat named
//   node launch/render.mjs --contact          a frame per beat in out/contact.png
//   node launch/render.mjs                    the clip: out/muster.mp4
//   node launch/render.mjs --sound            its sound again, on the picture already rendered
//
// The clip is 1920x1080 at 60 fps, each frame the blend (ffmpeg tmix) of 4
// subframes, or 8 where something moves more than 15 px a frame. Its sound
// is the kit's music from a downbeat and its effects, each placed so its
// measured peak lands on its beat, at -14 LUFS. Last, the video is scanned
// for single-frame pops.

import { spawn, spawnSync } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";
import { hidden } from "./check.mjs";
import { at, FADE, LENGTH, MUSIC, MUSIC_START, SOUNDS } from "./timeline.js";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = resolve(here, "..");
const out = join(here, "out");
const FPS = 60;
const FAST = 15;
const WORKERS = 3;
mkdirSync(out, { recursive: true });

const args = process.argv.slice(2);
const flag = (name) => {
  const index = args.indexOf(name);
  return index < 0 ? null : (args[index + 1] ?? "");
};

const types = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".png": "image/png",
  ".woff2": "font/woff2",
};
const server = createServer((request, response) => {
  const path = decodeURIComponent(new URL(request.url ?? "/", "http://x").pathname);
  const file = join(root, path);
  if (!file.startsWith(root) || !existsSync(file) || statSync(file).isDirectory()) {
    response.writeHead(404);
    response.end();
    return;
  }
  response.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(response);
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const url = `http://127.0.0.1:${server.address().port}/launch/index.html`;

const browser = await chromium.launch();
async function open() {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
  page.on("pageerror", (error) => console.error("page:", error.message));
  await page.goto(url);
  await page.evaluate(() => window.ready);
  return page;
}

async function still(page, t, path, type = "png") {
  await page.evaluate((time) => window.seek(time), t);
  return page.screenshot({ path, type, ...(type === "jpeg" ? { quality: 92 } : {}) });
}

function run(command, list, input) {
  const done = spawnSync(command, list, { input, maxBuffer: 1 << 30 });
  if (done.status !== 0) throw new Error(`${command} failed: ${done.stderr}`);
  return done;
}

const duration = at(LENGTH);
const frames = Math.round(duration * FPS);

if (flag("--stills") !== null) {
  const page = await open();
  for (const beat of flag("--stills").split(",").map(Number)) {
    const path = join(out, `still-beat-${beat}.png`);
    await still(page, at(beat + 0.5), path);
    console.log(path);
  }
} else if (args.includes("--contact")) {
  const page = await open();
  const folder = join(out, "contact");
  rmSync(folder, { recursive: true, force: true });
  mkdirSync(folder);
  // Each frame labelled with its beat, for the sheet only.
  await page.evaluate(() => {
    const label = document.createElement("div");
    label.id = "beat-label";
    Object.assign(label.style, { position: "fixed", left: "0", top: "0", zIndex: "99", padding: "6px 16px", font: "600 56px system-ui", color: "#ff0", background: "rgba(0,0,0,0.7)" });
    document.body.appendChild(label);
  });
  for (let beat = 0; beat < LENGTH; beat++) {
    await page.evaluate((b) => { document.getElementById("beat-label").textContent = String(b); }, beat);
    await still(page, at(beat + 0.5), join(folder, `${String(beat).padStart(3, "0")}.jpg`), "jpeg");
  }
  run("ffmpeg", [
    "-loglevel", "error", "-y", "-framerate", "1", "-i", join(folder, "%03d.jpg"),
    "-vf", `scale=384:216,tile=8x${Math.ceil(LENGTH / 8)}`,
    "-frames:v", "1", join(out, "contact.png"),
  ]);
  console.log(join(out, "contact.png"));
} else if (args.includes("--sound")) {
  mixSound();
  mux();
} else {
  const problems = hidden();
  if (problems.length) throw new Error(`hidden from view:\n${problems.join("\n")}`);
  await renderVideo();
  mixSound();
  mux();
  scanForPops();
}
await browser.close();
server.close();

/** How far anything moves between `t` and a frame later, at most, in pixels. */
async function speed(page, t) {
  return page.evaluate(
    ([now, later]) => {
      const a = window.seek(now);
      const b = window.seek(later);
      let most = 0;
      for (const key of Object.keys(a))
        if (b[key]) most = Math.max(most, Math.hypot(b[key][0] - a[key][0], b[key][1] - a[key][1]));
      return most;
    },
    [t, t + 1 / FPS],
  );
}

async function renderVideo() {
  const page = await open();
  // How many subframes each frame takes: 8 where anything moves fast.
  const counts = [];
  for (let frame = 0; frame < frames; frame++) counts.push((await speed(page, frame / FPS)) > FAST ? 8 : 4);
  await page.close();
  const segments = [];
  for (let frame = 0; frame < frames; frame++) {
    const last = segments.at(-1);
    if (last && last.count === counts[frame] && last.to - last.from < 300) last.to = frame + 1;
    else segments.push({ from: frame, to: frame + 1, count: counts[frame] });
  }
  console.log(`${frames} frames in ${segments.length} segments; ${counts.filter((c) => c === 8).length} at 8 subframes`);
  const folder = join(out, "segments");
  rmSync(folder, { recursive: true, force: true });
  mkdirSync(folder);
  let next = 0;
  const work = async () => {
    const worker = await open();
    while (next < segments.length) {
      const index = next++;
      const segment = segments[index];
      const n = segment.count;
      const file = join(folder, `${String(index).padStart(4, "0")}.mp4`);
      const encoder = spawn("ffmpeg", [
        "-loglevel", "error", "-y",
        "-f", "image2pipe", "-framerate", String(FPS * n), "-c:v", "mjpeg", "-i", "-",
        // JPEG frames are full range: blended, then brought to video range.
        "-vf", `tmix=frames=${n},select='eq(mod(n\\,${n})\\,${n - 1})',setpts=N/${FPS}/TB,scale=in_range=pc:out_range=tv,format=yuv420p`,
        "-r", String(FPS), "-c:v", "libx264", "-preset", "medium", "-crf", "12", "-pix_fmt", "yuv420p", file,
      ]);
      const finished = new Promise((done, fail) => encoder.on("close", (code) => (code === 0 ? done() : fail(new Error(`ffmpeg segment ${index}`)))));
      for (let frame = segment.from; frame < segment.to; frame++)
        for (let k = 0; k < n; k++) {
          const image = await still(worker, (frame + k / n) / FPS, undefined, "jpeg");
          if (!encoder.stdin.write(image)) await new Promise((done) => encoder.stdin.once("drain", done));
        }
      encoder.stdin.end();
      await finished;
      segment.file = file;
      process.stdout.write(`\rsegment ${index + 1}/${segments.length}`);
    }
    await worker.close();
  };
  await Promise.all(Array.from({ length: WORKERS }, work));
  console.log("");
  writeFileSync(join(out, "segments.txt"), segments.map((segment) => `file '${segment.file}'`).join("\n"));
  run("ffmpeg", ["-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", join(out, "segments.txt"), "-c", "copy", join(out, "picture.mp4")]);
}

/** Each effect's measured peak, in seconds from its start, from ../kit/AUDIO.md. */
function peaks() {
  const table = readFileSync(join(root, "kit/AUDIO.md"), "utf8");
  const found = {};
  for (const match of table.matchAll(/^\| `(\w+)\.mp3` \|.*\| (\d+) ms \| (\d+) ms \|$/gm))
    found[match[1]] = Number(match[3]) / 1000;
  return found;
}

/** The effects, each placed so its measured peak lands on its beat, after `first` inputs. */
function effects(first) {
  const audio = join(root, "kit/audio");
  const peak = peaks();
  const inputs = [];
  const chains = [];
  SOUNDS.forEach((sound, i) => {
    inputs.push("-i", join(audio, `${sound.sound}.mp3`));
    const offset = sound.slice ? 0 : (peak[sound.sound] ?? 0);
    const start = Math.max(0, at(sound.beat) - offset);
    const trim = sound.slice
      ? `atrim=start=${sound.slice.start}:duration=${at(sound.slice.beats)},asetpts=PTS-STARTPTS,afade=t=out:st=${Math.max(0, at(sound.slice.beats) - 0.08)}:d=0.08,`
      : "";
    chains.push(
      `[${i + first}:a]${trim}aformat=sample_rates=48000:channel_layouts=stereo,volume=${sound.gain}dB,adelay=${Math.round(start * 1000)}:all=1[s${i}]`,
    );
  });
  return { inputs, chains, labels: SOUNDS.map((_, i) => `[s${i}]`).join("") };
}

/** The integrated loudness of a file, and what loudnorm measured of it. */
function loudness(file) {
  const measured = run("ffmpeg", ["-hide_banner", "-i", file, "-af", "loudnorm=I=-14:TP=-1:LRA=11:print_format=json", "-f", "null", "-"]);
  return JSON.parse(String(measured.stderr).match(/\{[\s\S]*\}/)[0]);
}

function mixSound() {
  const audio = join(root, "kit/audio");
  // The music from its downbeat, under the effects: always mixed, as the
  // level the effects keep without it is the one they have over it.
  const music = ["-ss", String(MUSIC_START), "-t", String(duration), "-i", join(audio, "music.mp3")];
  const over = effects(1);
  const withMusic = join(out, "mix.wav");
  run("ffmpeg", [
    "-loglevel", "error", "-y", ...music, ...over.inputs, "-filter_complex",
    `[0:a]aformat=sample_rates=48000:channel_layouts=stereo,afade=t=out:st=${at(FADE[0])}:d=${at(FADE[1] - FADE[0])}[m];${over.chains.join(";")};[m]${over.labels}amix=inputs=${SOUNDS.length + 1}:normalize=0:duration=first[mix]`,
    "-map", "[mix]", "-t", String(duration), withMusic,
  ]);
  const stats = loudness(withMusic);
  if (MUSIC) {
    // Loudness to -14 LUFS: measured once, then set.
    run("ffmpeg", [
      "-loglevel", "error", "-y", "-i", withMusic, "-af",
      `loudnorm=I=-14:TP=-1:LRA=11:measured_I=${stats.input_i}:measured_TP=${stats.input_tp}:measured_LRA=${stats.input_lra}:measured_thresh=${stats.input_thresh}:offset=${stats.target_offset}:linear=true`,
      "-ar", "48000", join(out, "sound.wav"),
    ]);
  } else {
    // The effects alone, raised by what the mix with music needed to reach
    // -14 LUFS, and held under -1 dBTP.
    const gain = -14 - Number(stats.input_i);
    const alone = effects(1);
    run("ffmpeg", [
      "-loglevel", "error", "-y", "-f", "lavfi", "-t", String(duration), "-i", "anullsrc=r=48000:cl=stereo", ...alone.inputs,
      "-filter_complex", `${alone.chains.join(";")};[0:a]${alone.labels}amix=inputs=${SOUNDS.length + 1}:normalize=0:duration=first,volume=${gain.toFixed(2)}dB,alimiter=limit=0.891:level=false[mix]`,
      "-map", "[mix]", "-t", String(duration), "-ar", "48000", join(out, "sound.wav"),
    ]);
  }
  const check = loudness(join(out, "sound.wav"));
  console.log(`sound${MUSIC ? "" : " (effects only)"}: ${check.input_i} LUFS, peak ${check.input_tp} dBTP`);
}

function mux() {
  run("ffmpeg", [
    "-loglevel", "error", "-y", "-i", join(out, "picture.mp4"), "-i", join(out, "sound.wav"),
    "-vf", "scale=out_range=tv,format=yuv420p",
    "-c:v", "libx264", "-preset", "slow", "-crf", "17", "-pix_fmt", "yuv420p", "-color_range", "tv", "-r", String(FPS),
    "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", "-shortest", join(out, "muster.mp4"),
  ]);
  console.log(join(out, "muster.mp4"));
}

/**
 * A single-frame pop: a frame far from both its neighbours while they are
 * close to each other. Scanned on small grey frames.
 */
function scanForPops() {
  const width = 192;
  const height = 108;
  const raw = run("ffmpeg", ["-loglevel", "error", "-i", join(out, "muster.mp4"), "-vf", `scale=${width}:${height},format=gray`, "-f", "rawvideo", "-"]).stdout;
  const size = width * height;
  const count = Math.floor(raw.length / size);
  const distance = (a, b) => {
    let sum = 0;
    for (let i = 0; i < size; i++) sum += Math.abs(raw[a * size + i] - raw[b * size + i]);
    return sum / size;
  };
  const pops = [];
  for (let frame = 1; frame < count - 1; frame++) {
    const before = distance(frame - 1, frame);
    const after = distance(frame, frame + 1);
    const across = distance(frame - 1, frame + 1);
    const score = Math.min(before, after) - across;
    if (score > 2) pops.push({ frame, second: (frame / FPS).toFixed(3), beat: (frame / FPS / at(1)).toFixed(2), score: score.toFixed(2) });
  }
  writeFileSync(join(out, "pops.json"), `${JSON.stringify(pops, null, 2)}\n`);
  console.log(pops.length ? `pops: ${JSON.stringify(pops)}` : "no single-frame pops");
}
