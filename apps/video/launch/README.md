# launch

## Overview

The clip: Muster's desktop app, from the problem it solves to each thing
it does, cut on a beat grid. Its rules are [CLAUDE.md](CLAUDE.md), its look
[LOOK.md](LOOK.md); everything it shows of the app comes from
[`../kit`](../kit/README.md).

## Interface

- `index.html`: the stage (1920x1080) and `window.seek(t)`, which sets every element for the time `t` in seconds and returns where each moving thing is; `window.ready` resolves once the faces and shots are loaded.
- `timeline.js`: every time, in beats (123.97 BPM, a beat 0.48399 s): the opening lines and cards, the reveal, each scene's headline and the side it takes (`place`), the shots the window shows, the camera's moves, rings, clicks, the ending, and the sounds. `MUSIC` turns the music on or off (off in this cut).
- `engine.js`: the motion: springs in closed form, the camera (one eased move at a time, zoom in log space) and the layouts it frames shots in, the pointer.
- `scenes.js`: builds the elements once and returns `seek`.
- `check.mjs` (`pnpm check`): every click and ring lands inside its scene's free region, clear of the headline; the render refuses to start otherwise.
- `render.mjs`: `--stills 9,17,45`, `--contact`, `--sound`, or the whole clip: subframes captured in Chromium, blended with ffmpeg `tmix` (4 a frame, 8 where anything moves over 15 px), the sound mixed to the beats, then a scan for single-frame pops. Writes `out/`.
- `media.sh` (`pnpm media`): the README's GIF and MP4, in `docs/media`.

## Invariants

- No CSS animation, transition, timer or `requestAnimationFrame`; nothing is kept between frames.
- Every time is in `timeline.js`; `engine.js` and `scenes.js` hold none.
- Product pixels only from `../kit/shots`, at the positions in `../kit/crops.js`.
- Each effect's measured peak lands on its beat.

## Side Effects

- Writes `out/`: stills, `contact.png`, segments, `picture.mp4`, `sound.wav`, `muster.mp4`, `pops.json`.
- Serves the folder above on a free local port while rendering.

## Verification

- `pnpm check`; `pnpm contact`, then look at `out/contact.png`; after `pnpm render`, `out/pops.json` is empty.
