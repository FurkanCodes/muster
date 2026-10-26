// Checks the cut before it is rendered: every click, and every ring, lands
// inside the region its scene leaves the window, clear of the headline.
// Run by render.mjs; exits non-zero with what is hidden.

import { crops, points } from "../kit/crops.js";
import { LAYOUTS, layoutAt, onScreen } from "./engine.js";
import { at, CLICKS, RINGS } from "./timeline.js";

const inside = (point, box, slack = 0) =>
  point.x >= box.x0 - slack && point.x <= box.x1 + slack && point.y >= box.y0 - slack && point.y <= box.y1 + slack;

export function hidden() {
  const problems = [];
  const look = (what, beat, corners) => {
    const layout = LAYOUTS[layoutAt(beat)];
    for (const point of corners) {
      if (!inside(point, { x0: 0, x1: 1920, y0: 0, y1: 1080 })) problems.push(`${what} at beat ${beat} is off screen`);
      else if (layout.text && inside(point, layout.text)) problems.push(`${what} at beat ${beat} is under the ${layoutAt(beat)} headline`);
    }
  };
  for (const click of CLICKS) look(`click on ${click.point}`, click.beat, [onScreen(points[click.point], at(click.beat))]);
  for (const ring of RINGS) {
    const box = crops[ring.crop];
    const when = at(ring.beat + 0.5);
    look(`ring on ${ring.crop}`, ring.beat, [
      onScreen({ x: box.x, y: box.y }, when),
      onScreen({ x: box.x + box.w, y: box.y + box.h }, when),
    ]);
  }
  return [...new Set(problems)];
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const problems = hidden();
  for (const problem of problems) console.log(problem);
  console.log(problems.length ? `${problems.length} hidden` : "every click and ring is in view");
  process.exit(problems.length ? 1 : 0);
}
