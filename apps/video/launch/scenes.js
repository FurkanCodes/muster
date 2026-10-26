// The clip's elements, made once, and `seek(t)`, which sets every one from
// the time alone. Product pixels are only ever the kit's shots and crops.

import { between, clamp, makeEngine, spring, springs } from "./engine.js";
import { at } from "./timeline.js";

const COLOURS = {
  waiting: "#ef7e6b",
  working: "#d4b06f",
  done: "#7fbf8f",
  keys: "#4f8fff",
  text: "#e8e6e1",
  quiet: "#9b988f",
  term: "#111110",
};

function element(tag, className, parent, text) {
  const made = document.createElement(tag);
  if (className) made.className = className;
  if (text !== undefined) made.textContent = text;
  parent.appendChild(made);
  return made;
}

/** A crop of a shot of `kit` (found at `folder`), drawn at `scale` CSS pixels per window pixel. */
function crop(kit, folder, name, parent, scale = 1) {
  const found = kit.crops[name];
  if (!found) throw new Error(`no crop ${name} in ${folder}/crops.js`);
  const shot = kit.shots[found.shot];
  const made = element("div", "crop", parent);
  Object.assign(made.style, {
    width: `${found.w * scale}px`,
    height: `${found.h * scale}px`,
    backgroundImage: `url(../${folder}/shots/${shot.file})`,
    backgroundSize: `${shot.width * scale}px ${shot.height * scale}px`,
    backgroundPosition: `${-found.x * scale}px ${-found.y * scale}px`,
  });
  return made;
}

/** A line of words, each its own span, the accent word coloured. */
function words(parent, list, accent, colour) {
  return list.map((word) => {
    const span = element("span", "word", parent, word);
    if (accent && accent.split(" ").includes(word.replace(/[.,]$/, "")) || word === accent)
      span.style.color = COLOURS[colour];
    return span;
  });
}

const style = (node, values) => Object.assign(node.style, values);
const px = (value) => `${value.toFixed(2)}px`;

/**
 * Builds cut `cut` (a timeline module) from kit `kit` (its crops module) on
 * `stage`, and returns its `seek`.
 */
export function build(stage, cut, kit) {
  const { ACT1, HEADLINES, KEYS, OUTRO, REVEAL, RINGS, SSH_ROWS, TYPED, WINDOW_IN, WINDOW_SHOTS } = cut;
  const { crops, shots } = kit;
  const { camera, onScreen, pointer, WINDOW } = makeEngine(cut, kit);
  const folder = cut.KIT;
  const cropOf = (name, parent, scale) => crop(kit, folder, name, parent, scale);

  const hud = element("div", "hud", stage);
  const world = element("div", "world", stage);
  const frame = element("div", "window", world);
  style(world, { width: `${WINDOW.width}px`, height: `${WINDOW.height}px` });
  style(frame, { width: `${WINDOW.width}px`, height: `${WINDOW.height}px`, background: cut.LIGHTS ? "#171716" : COLOURS.term });
  if (cut.LIGHTS) {
    const lights = element("div", "lights", frame);
    for (const colour of ["#ff5f57", "#febc2e", "#28c840"]) element("i", "", lights).style.background = colour;
  }

  // The window: every shot it shows, stacked; the one shown on top.
  const layers = new Map();
  for (const [, name] of WINDOW_SHOTS) {
    if (layers.has(name)) continue;
    const image = element("img", "shot", frame);
    image.src = `../${folder}/shots/${shots[name].file}`;
    style(image, { width: `${WINDOW.width}px`, height: `${WINDOW.height}px` });
    layers.set(name, image);
  }
  // What is typed, uncovered a letter at a time: the crop of the text from
  // the shot after it was typed, over the shot before.
  const typings = (TYPED ? [TYPED].flat() : []).map((typing) => {
    const made = cropOf(typing.crop, frame);
    made.classList.add("over");
    return { made, typing };
  });
  const ssh = SSH_ROWS ? crops[SSH_ROWS.crop] : null;
  const masks = SSH_ROWS ? Array.from({ length: SSH_ROWS.rows + 1 }, () => element("div", "mask", frame)) : [];
  for (const mask of masks) mask.style.background = COLOURS.term;

  // Act 1.
  const act = element("div", "act", hud);
  const line1 = ACT1 && element("div", "line line1", act);
  const first = ACT1 ? words(line1, ACT1.line1.words, ACT1.line1.accent, ACT1.line1.colour) : [];
  const line2 = ACT1 && element("div", "line line2", act);
  if (ACT1) words(line2, ACT1.line2.text.split(" "), ACT1.line2.accent, ACT1.line2.colour);
  const cards = (ACT1?.cards ?? []).map((card) => {
    const holder = element("div", "card", act);
    cropOf(card.crop, holder, 2.2);
    return holder;
  });
  const badge = ACT1 && element("div", "badge", act);
  if (ACT1) cropOf(ACT1.badge.crop, badge, 1.75);
  const dot = ACT1 && element("div", "dot", act);
  const question = ACT1 && element("div", "question", act, ACT1.question.text);

  // The drop.
  const reveal = element("div", "reveal", hud);
  const logo = element("img", "logo", reveal);
  logo.src = `../${folder}/shots/kep.png`;
  const name = element("div", "name", reveal, "Muster");
  const tagline = element("div", "tagline", reveal, cut.TAGLINE);

  // Headlines over the window, on a band that keeps them clear of it.
  const headlines = HEADLINES.map((headline) => {
    const band = element("div", `band band-${headline.place}`, hud);
    const holder = element("div", `headline headline-${headline.place}`, hud);
    const title = element("div", "title", holder);
    const parts = words(title, headline.title.split(" "), headline.accent, headline.colour);
    const sub = element("div", "sub", holder, headline.sub);
    // The keys pressed in this scene, each with what the terminal UI's own
    // hints call it, one shown at a time under the headline.
    const pressed = element("div", "keys", holder);
    const keys = KEYS.filter((key) => key.beat >= headline.beat && key.beat < headline.out).map((key) => {
      const made = element("div", "key", pressed);
      for (const cap of key.keys) element("span", "cap", made, cap);
      element("span", "hint", made, key.hint);
      return { made, key };
    });
    if (!keys.length) pressed.remove();
    return { band, holder, parts, sub, headline, keys };
  });
  const rings = RINGS.map((ring) => {
    const made = element("div", "ring", hud);
    made.style.borderColor = COLOURS[ring.colour];
    made.style.boxShadow = `0 0 24px ${COLOURS[ring.colour]}55`;
    return made;
  });
  const arrow = element("div", "pointer", hud);
  arrow.innerHTML =
    '<svg width="34" height="34" viewBox="0 0 24 24"><path d="M5 3 L5 20 L9.5 15.5 L12.5 22 L15.2 20.8 L12.3 14.4 L18.5 14.4 Z" fill="#fff" stroke="#000" stroke-width="1.3" stroke-linejoin="round"/></svg>';
  const ripple = element("div", "ripple", hud);

  // The ending.
  const outro = element("div", "outro", hud);
  const outroLogo = element("img", "logo", outro);
  outroLogo.src = `../${folder}/shots/kep.png`;
  const outroName = element("div", "name", outro, "Muster");
  const install = element("div", "install", outro);
  install.innerHTML =
    '<span class="prompt">$</span> curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/main/install.sh | sh';
  const url = element("div", "url", outro, "github.com/FurkanCodes/muster · macOS and Linux · MIT");

  return function seek(t) {
    // Where each moving thing is, by name, so frames can be compared.
    const marks = {};
    const mark = (key, x, y, size = 0) => {
      marks[`${key}.a`] = [x, y];
      marks[`${key}.b`] = [x + size, y + size];
    };
    const beat = t / at(1);

    // Act 1: the lines, the rows, the squeeze, the dot.
    style(act, { display: ACT1 && beat < WINDOW_IN + 0.5 ? "block" : "none" });
    if (ACT1) {
      const squeeze = spring(t, at(ACT1.squeeze), { omega: 9, zeta: 0.8 });
      const up = spring(t, at(ACT1.up));
      const linesGone = between(t, ACT1.squeeze, ACT1.squeeze + 0.8);
      style(line1, { top: px(330 - 210 * up - 60 * linesGone), opacity: 1 - linesGone });
      first.forEach((span, i) => {
        const k = spring(t, at(ACT1.line1.beats[i]));
        style(span, { opacity: clamp(k * 1.6), transform: `translateY(${px(60 * (1 - k))})` });
        mark(`line1.${i}`, 960, 330 - 210 * up + 60 * (1 - k), 120);
      });
      const rise2 = spring(t, at(ACT1.line2.beat));
      style(line2, {
        top: px(262 + 70 * (1 - rise2) - 60 * linesGone),
        opacity: clamp(rise2 * 1.6) * (1 - linesGone),
      });
      const centre = { x: 960, y: 560 };
      cards.forEach((card, i) => {
        const k = spring(t, at(ACT1.cards[i].beat));
        const home = { x: 960, y: 420 + i * 140 + 63 };
        const x = home.x + (centre.x - home.x) * squeeze;
        const y = home.y + 70 * (1 - k) + (centre.y - home.y) * squeeze;
        const scale = (0.94 + 0.06 * k) * (1 - 0.97 * squeeze);
        style(card, {
          left: px(x),
          top: px(y),
          opacity: clamp(k * 1.5) * (1 - between(t, ACT1.squeeze + 0.5, ACT1.squeeze + 1)),
          transform: `translate(-50%, -50%) scale(${scale.toFixed(4)})`,
        });
        mark(`card.${i}`, x, y, 270 * scale);
      });
      const tag = spring(t, at(ACT1.badge.beat), { omega: 16, zeta: 0.6 });
      const tagHome = { x: 1150, y: 420 + 3 * 140 + 120 };
      style(badge, {
        left: px(tagHome.x + (centre.x - tagHome.x) * squeeze),
        top: px(tagHome.y + (centre.y - tagHome.y) * squeeze),
        opacity: clamp(tag * 2) * (1 - between(t, ACT1.squeeze + 0.5, ACT1.squeeze + 1)),
        transform: `translate(-50%, -50%) rotate(-3deg) scale(${((0.6 + 0.4 * tag) * (1 - 0.97 * squeeze)).toFixed(4)})`,
      });
      const dotIn = spring(t, at(ACT1.dot), { omega: 16, zeta: 0.55 });
      const pulse = springs(t, 1, ACT1.pulses.flatMap((b) => [[b, 1.5], [b + 0.4, 1]]), { omega: 20, zeta: 0.6 });
      // On the drop the dot bursts outward and is gone as Kep lands.
      const burst = spring(t, at(REVEAL.logo), { omega: 12, zeta: 0.9 });
      const dotScale = dotIn * pulse * (1 + 26 * burst);
      style(dot, {
        transform: `translate(-50%, -50%) scale(${dotScale.toFixed(4)})`,
        opacity: 1 - between(t, REVEAL.logo, REVEAL.logo + 0.45),
      });
      mark("dot", 960, 560, 18 * dotScale);
      const asked = spring(t, at(ACT1.question.beat));
      style(question, {
        opacity: clamp(asked * 1.5) * (1 - between(t, ACT1.question.out, ACT1.question.out + 0.4)),
        transform: `translate(-50%, 0) translateY(${px(30 * (1 - asked))})`,
      });
    }

    // The drop: Kep, the name, what it is.
    const kep = spring(t, at(REVEAL.logo), { omega: 15, zeta: 0.6 });
    const named = spring(t, at(REVEAL.name));
    const told = spring(t, at(REVEAL.tagline));
    const gone = between(t, REVEAL.out, REVEAL.out + 0.6);
    style(reveal, { display: beat >= REVEAL.logo - 0.1 && beat < REVEAL.out + 1 ? "flex" : "none", opacity: 1 - gone, transform: `translateY(${px(-80 * gone)})` });
    style(logo, { transform: `scale(${kep.toFixed(4)})`, opacity: clamp(kep * 2) });
    style(name, { opacity: clamp(named * 1.5), transform: `translateY(${px(40 * (1 - named))})` });
    style(tagline, { opacity: clamp(told * 1.5), transform: `translateY(${px(30 * (1 - told))})` });
    mark("reveal", 960, 400 - 80 * gone, 220 * kep);

    // The window, under the camera.
    const view = camera(t);
    const shown = beat >= WINDOW_IN - 0.5 && beat < OUTRO.logo + 1.5 ? 1 - between(t, OUTRO.logo - 0.6, OUTRO.logo) : 0;
    style(world, {
      opacity: (shown * clamp((t - at(WINDOW_IN)) / at(0.4))).toFixed(4),
      transform: `translate(${px(view.ax)}, ${px(view.ay)}) scale(${view.zoom.toFixed(5)}) translate(${px(-view.x)}, ${px(-view.y)})`,
    });
    const topLeft = onScreen({ x: 0, y: 0 }, t);
    const bottomRight = onScreen({ x: WINDOW.width, y: WINDOW.height }, t);
    mark("window.topleft", topLeft.x, topLeft.y);
    mark("window.bottomright", bottomRight.x, bottomRight.y);
    let current = WINDOW_SHOTS[0];
    let previous = null;
    for (const entry of WINDOW_SHOTS) if (beat >= entry[0]) [previous, current] = [current, entry];
    const fade = previous ? clamp((t - at(current[0])) / 0.07) : 1;
    for (const [shotName, image] of layers) {
      const isCurrent = shotName === current[1];
      const isPrevious = previous && shotName === previous[1] && fade < 1;
      style(image, {
        display: isCurrent || isPrevious ? "block" : "none",
        zIndex: isCurrent ? 2 : 1,
        opacity: isCurrent ? fade.toFixed(4) : 1,
      });
    }

    // What is typed, a letter at a time.
    for (const { made, typing } of typings) {
      const area = crops[typing.crop];
      const letters = typing.letters ?? 27;
      const typed = clamp((t - at(typing.from)) / (at(typing.to) - at(typing.from)));
      const full = typing.width ?? area.w;
      const lead = typing.lead ?? 10;
      const width = lead + (full - lead) * (Math.floor(typed * letters) / letters);
      style(made, {
        display: beat >= typing.from && beat < typing.until ? "block" : "none",
        left: px(area.x),
        top: px(area.y),
        clipPath: `inset(0 ${px(area.w - width)} 0 0)`,
        zIndex: 3,
      });
    }

    // `muster ssh`, a row at a time; the typed rows a letter at a time.
    if (SSH_ROWS) {
      const rowHeight = ssh.h / SSH_ROWS.rows;
      masks.forEach((mask, i) => {
        const row = Math.min(i, SSH_ROWS.rows - 1);
        const start = SSH_ROWS.beats[row];
        const typedRow = SSH_ROWS.typed.includes(i) && i < SSH_ROWS.rows;
        const progress = typedRow
          ? Math.floor(clamp((t - at(start)) / at(1)) * 24) / 24
          : beat >= start
            ? 1
            : 0;
        const last = i === SSH_ROWS.rows;
        style(mask, {
          display: current[1] === "ssh" && (last ? beat < SSH_ROWS.beats.at(-1) + 1 : progress < 1) ? "block" : "none",
          left: px(ssh.x - 6 + (ssh.w + 12) * (last ? 0 : progress)),
          top: px(ssh.y + i * rowHeight),
          width: px((ssh.w + 12) * (last ? 1 : 1 - progress) + (last ? 24 : 0)),
          height: px(rowHeight + (last ? 8 : 0.5)),
          zIndex: 3,
        });
      });
    }

    // Headlines, and the band under them.
    for (const { band, holder, parts, sub, headline, keys } of headlines) {
      const out = between(t, headline.out, headline.out + 0.4);
      const visible = beat >= headline.beat - 0.1 && beat < headline.out + 0.5;
      // The band under the text comes in as the window moves over to its side.
      const shade = between(t, headline.beat - 0.4, headline.beat + 0.4) * (1 - out);
      style(band, { display: visible ? "block" : "none", opacity: shade.toFixed(3) });
      style(holder, { display: visible ? "block" : "none", opacity: 1 - out, translate: `0 ${px(-24 * out)}` });
      if (!visible) continue;
      parts.forEach((span, i) => {
        const k = spring(t, at(headline.beat) + i * 0.06);
        style(span, { opacity: clamp(k * 1.6), transform: `translateY(${px(36 * (1 - k))})` });
        mark(`headline.${headline.beat}.${i}`, 200 + i * 120, 70 + 36 * (1 - k), 60);
      });
      const said = spring(t, at(headline.beat + 0.35));
      style(sub, { opacity: clamp(said * 1.5), transform: `translateY(${px(20 * (1 - said))})` });
      // A key shows from its press until the next one, or two beats.
      keys.forEach(({ made, key }, i) => {
        const until = Math.min(keys[i + 1]?.key.beat ?? Infinity, key.beat + 2.2);
        const on = spring(t, at(key.beat - 0.12), { omega: 18, zeta: 0.6 });
        const off = between(t, until - 0.2, until);
        const showing = beat >= key.beat - 0.15 && beat < until;
        style(made, {
          display: showing ? "flex" : "none",
          opacity: (clamp(on * 2) * (1 - off)).toFixed(3),
          transform: `translateY(${px(14 * (1 - on))}) scale(${(0.9 + 0.1 * on).toFixed(4)})`,
        });
        if (showing) mark(`key.${key.beat}`, 300, 300 + 14 * (1 - on), 60 * on);
      });
    }

    // Rings round what to look at, on screen so their line keeps its width.
    RINGS.forEach((ring, i) => {
      const area = crops[ring.crop];
      const on = spring(t, at(ring.beat), { omega: 16, zeta: 0.6 });
      const off = between(t, ring.out, ring.out + 0.3);
      const a = onScreen({ x: area.x - 10, y: area.y - 10 }, t);
      const b = onScreen({ x: area.x + area.w + 10, y: area.y + area.h + 10 }, t);
      const grow = 1 + 0.08 * (1 - on);
      style(rings[i], {
        display: beat >= ring.beat && beat < ring.out + 0.4 ? "block" : "none",
        left: px(a.x),
        top: px(a.y),
        width: px(b.x - a.x),
        height: px(b.y - a.y),
        opacity: (clamp(on * 1.5) * (1 - off)).toFixed(3),
        transform: `scale(${grow.toFixed(4)})`,
      });
    });

    // The pointer.
    const at_ = pointer(t);
    if (at_) {
      const tip = onScreen(at_, t);
      style(arrow, { display: "block", left: px(tip.x - 6), top: px(tip.y - 4), opacity: at_.opacity.toFixed(3), transform: `scale(${at_.press.toFixed(3)})` });
      style(ripple, {
        display: at_.ring === null ? "none" : "block",
        left: px(tip.x - 34),
        top: px(tip.y - 34),
        opacity: at_.ring === null ? 0 : (0.75 * (1 - at_.ring)).toFixed(3),
        transform: `scale(${at_.ring === null ? 0 : (0.25 + 0.95 * at_.ring).toFixed(3)})`,
      });
      mark("pointer", tip.x, tip.y);
    } else {
      style(arrow, { display: "none" });
      style(ripple, { display: "none" });
    }

    // The ending.
    const ending = spring(t, at(OUTRO.logo), { omega: 14, zeta: 0.62 });
    const how = spring(t, at(OUTRO.install));
    const where = spring(t, at(OUTRO.url));
    style(outro, { display: beat >= OUTRO.logo - 0.1 ? "flex" : "none" });
    style(outroLogo, { transform: `scale(${ending.toFixed(4)})`, opacity: clamp(ending * 2) });
    style(outroName, { opacity: clamp(ending * 1.5), transform: `translateY(${px(30 * (1 - ending))})` });
    style(install, { opacity: clamp(how * 1.5), transform: `translateY(${px(24 * (1 - how))})` });
    style(url, { opacity: clamp(where * 1.5), transform: `translateY(${px(20 * (1 - where))})` });
    mark("outro", 960, 380, 200 * ending);

    return marks;
  };
}

