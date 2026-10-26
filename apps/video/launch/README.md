# launch

## Overview

The videos, as cuts on one beat grid: the desktop app's (`timeline.js`),
from the problem Muster solves to each thing it does, and the terminal
UI's (`timeline-tui.js`), the same things done by key. Their rules are
[CLAUDE.md](CLAUDE.md), their look [LOOK.md](LOOK.md); everything they
show of the apps comes from [`../kit`](../kit/README.md) and
[`../kit/tui`](../kit/tui/README.md).

## Interface

- `index.html`: the stage (1920x1080) and `window.seek(t)`, which sets every element for the time `t` in seconds and returns where each moving thing is; `window.ready` resolves once the faces and shots are loaded. `?cut=tui` plays the terminal UI's cut.
- `cut.js`: `loadCut(name)`: a cut's timeline and the kit it names (`KIT`); the page, the check and the renderer load cuts through it.
- `timeline.js`: the desktop app's cut, and the tempo both share (123.97 BPM, a beat 0.48399 s, `at`): the opening lines and cards, the reveal, each scene's headline and the side it takes (`place`), the shots the window shows, the camera's moves, rings, clicks, the ending, the sounds. `MUSIC` turns the music on or off (off in both cuts).
- `timeline-tui.js`: the terminal UI's cut: no opening act and no pointer; `KEYS`, each key pressed and what the terminal UI's hints call it, shown under the headline; `TYPED`, a list (the branch, then the prompt).
- `engine.js`: the motion: springs in closed form, the layouts each scene's headline leaves the window, and `makeEngine(cut, kit)`: the camera (one eased move at a time, zoom in log space, framed in the region of the scene it arrives in), the pointer. The window is the size of the cut's shots.
- `scenes.js`: `build(stage, cut, kit)`: makes a cut's elements once and returns its `seek`.
- `check.mjs` (`pnpm check [tui]`): every click and ring lands inside its scene's free region, clear of the headline; the render refuses to start otherwise.
- `render.mjs`: `--cut tui`, then `--stills 9,17,45`, `--contact`, `--sound`, or the whole video: subframes captured in Chromium, blended with ffmpeg `tmix` (4 a frame, 8 where anything moves over 15 px), the sound mixed to the beats, then a scan for single-frame pops. Writes `out/` (the terminal UI's in `out/tui/`).
- `media.sh` (`pnpm media [desktop|tui]`): the README's GIF and MP4 of a cut, in `docs/media`.

## Invariants

- No CSS animation, transition, timer or `requestAnimationFrame`; nothing is kept between frames.
- Every time is in a cut's timeline; `engine.js` and `scenes.js` hold none.
- Product pixels only from a kit's shots, at the positions in its `crops.js`.
- Each effect's measured peak lands on its beat.

## Side Effects

- Writes `out/` (and `out/tui/`): stills, `contact.png`, segments, `picture.mp4`, `sound.wav`, `muster.mp4`, `pops.json`.
- Serves the folder above on a free local port while rendering.

## Verification

- `pnpm check` and `pnpm check tui`; `pnpm contact [--cut tui]`, then look at the sheet; after rendering, `pops.json` is empty.
