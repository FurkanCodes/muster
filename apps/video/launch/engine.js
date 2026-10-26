// The clip's motion, each a pure function of time: springs in closed form,
// eased camera moves, the pointer's path. Nothing here keeps state.

import { crops, points } from "../kit/crops.js";
import { at, CAMERA, CLICKS, HEADLINES } from "./timeline.js";

export const SCREEN = { width: 1920, height: 1080 };
export const WINDOW = { width: 1440, height: 900 };

/**
 * Where a scene's headline sits, and the part of the screen it leaves the
 * window: the camera frames every shot inside that region, so nothing the
 * scene shows or clicks is ever under the text. `text` is the headline's box.
 */
export const LAYOUTS = {
  centre: { region: { x0: 120, x1: 1800, y0: 120, y1: 1000 } },
  top: { region: { x0: 120, x1: 1800, y0: 250, y1: 1040 }, text: { x0: 0, x1: 1920, y0: 0, y1: 230 } },
  bottom: { region: { x0: 120, x1: 1800, y0: 40, y1: 800 }, text: { x0: 0, x1: 1920, y0: 830, y1: 1080 } },
  left: { region: { x0: 720, x1: 1880, y0: 60, y1: 1020 }, text: { x0: 0, x1: 690, y0: 0, y1: 1080 } },
  right: { region: { x0: 40, x1: 1200, y0: 60, y1: 1020 }, text: { x0: 1230, x1: 1920, y0: 0, y1: 1080 } },
};

/** The layout of the scene shown at `beat`: its headline's, or the centre's between scenes. */
export function layoutAt(beat) {
  const headline = HEADLINES.find((found) => beat >= found.beat && beat < found.out + 0.5);
  return headline ? headline.place : "centre";
}

const anchorOf = (region) => ({ x: (region.x0 + region.x1) / 2, y: (region.y0 + region.y1) / 2 });

/**
 * A spring's step from 0 to 1, begun at `start` seconds: closed form,
 * underdamped, so it overshoots a little (about 5 %) and settles in about
 * a beat.
 */
export function spring(t, start, { omega = 13, zeta = 0.7 } = {}) {
  const tau = t - start;
  if (tau <= 0) return 0;
  const damped = omega * Math.sqrt(1 - zeta * zeta);
  const decay = Math.exp(-zeta * omega * tau);
  return 1 - decay * (Math.cos(damped * tau) + ((zeta * omega) / damped) * Math.sin(damped * tau));
}

/**
 * A value that changes target more than once: `first`, then each
 * `[beat, value]` in turn, as a sum of springs, one per change.
 */
export function springs(t, first, changes, options) {
  let value = first;
  let previous = first;
  for (const [beat, target] of changes) {
    value += (target - previous) * spring(t, at(beat), options);
    previous = target;
  }
  return value;
}

export const ease = (x) => (x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2);
export const clamp = (value, low = 0, high = 1) => Math.min(high, Math.max(low, value));
/** 0 before `from`, 1 after `to`, eased between (beats). */
export const between = (t, from, to) => ease(clamp((t - at(from)) / (at(to) - at(from))));

/** The middle of a crop, in the window's pixels. */
export function middle(name) {
  const crop = crops[name];
  if (!crop) throw new Error(`no crop ${name} in kit/crops.js`);
  return { x: crop.x + crop.w / 2, y: crop.y + crop.h / 2 };
}

/**
 * A camera key's aim, framed in the region of the scene it arrives in:
 * the aimed point goes to the region's middle; zoomed past the region's
 * size, the window still covers the region, with no gap at its edges;
 * `left` holds its left edge, and the list, at the region's left, and
 * `top` its top edge, and the title line, at the region's top.
 */
function aim(key) {
  const { region } = LAYOUTS[key.place ?? layoutAt(key.to)];
  const anchor = anchorOf(region);
  const point = key.on ? middle(key.on) : { x: key.x, y: key.y };
  let x = point.x + (key.dx ?? 0);
  let y = point.y + (key.dy ?? 0);
  const { zoom } = key;
  const low = (anchor.x - region.x0) / zoom;
  const high = WINDOW.width - (region.x1 - anchor.x) / zoom;
  x = low <= high ? (key.left ? low : clamp(x, low, high)) : key.on ? clamp(x, high, low) : WINDOW.width / 2;
  const top = (anchor.y - region.y0) / zoom;
  const bottom = WINDOW.height - (region.y1 - anchor.y) / zoom;
  y = top <= bottom ? (key.top ? top : clamp(y, top, bottom)) : key.on ? clamp(y, bottom, top) : WINDOW.height / 2;
  return { x, y, zoom, ax: anchor.x, ay: anchor.y };
}

const KEYS = CAMERA.map((key) => ({ ...key, ...aim(key) }));
for (let i = 1; i < KEYS.length; i++)
  if (KEYS[i].from < KEYS[i - 1].to) throw new Error(`camera moves ${i - 1} and ${i} overlap`);

/**
 * Where the camera looks at `t`, and where on screen that point sits: one
 * eased move at a time, the zoom in log space.
 */
export function camera(t) {
  let state = KEYS[0];
  for (const key of KEYS.slice(1)) {
    if (t >= at(key.to)) {
      state = key;
      continue;
    }
    if (t > at(key.from)) {
      const k = between(t, key.from, key.to);
      const mix = (name) => state[name] + (key[name] - state[name]) * k;
      return {
        x: mix("x"),
        y: mix("y"),
        ax: mix("ax"),
        ay: mix("ay"),
        zoom: Math.exp(Math.log(state.zoom) + (Math.log(key.zoom) - Math.log(state.zoom)) * k),
      };
    }
    break;
  }
  return { x: state.x, y: state.y, ax: state.ax, ay: state.ay, zoom: state.zoom };
}

/** A point in the window, on screen, at `t`. */
export function onScreen(point, t) {
  const view = camera(t);
  return { x: view.ax + (point.x - view.x) * view.zoom, y: view.ay + (point.y - view.y) * view.zoom };
}

/**
 * The pointer at `t`, in the window's pixels: it comes in a beat before a
 * click, moves to it, clicks on the beat, and goes; between clicks close
 * together it goes straight from one to the next.
 */
export function pointer(t) {
  for (let i = 0; i < CLICKS.length; i++) {
    const click = CLICKS[i];
    const target = points[click.point];
    if (!target) throw new Error(`no point ${click.point} in kit/crops.js`);
    if (t > at(click.beat + 0.7)) continue;
    const before = CLICKS[i - 1];
    const near = before && click.beat - before.beat < 2.6;
    const start = near ? points[before.point] : { x: target.x + 150, y: target.y + 110 };
    const shown = near ? 1 : clamp((t - at(click.beat - 1.2)) / at(0.25));
    if (shown <= 0) return null;
    const k = between(t, click.beat - 1.0, click.beat - 0.12);
    const since = t - at(click.beat);
    const leaving = clamp((t - at(click.beat + 0.45)) / at(0.25));
    const next = CLICKS[i + 1];
    const staying = next && next.beat - click.beat < 2.6;
    return {
      x: start.x + (target.x - start.x) * k,
      y: start.y + (target.y - start.y) * k,
      opacity: shown * (staying ? 1 : 1 - leaving),
      press: since >= 0 && since < 0.12 ? 1 - 0.16 * Math.sin((since / 0.12) * Math.PI) : 1,
      ring: since >= 0 && since < 0.32 ? since / 0.32 : null,
    };
  }
  return null;
}
