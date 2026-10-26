# video

## Overview

The short videos at the top of the repository's README, one for each way
to use Muster, cut on the same beat grid:

- **The desktop app:** its real window, screenshots of the app running over its pretend daemon (`apps/desktop/src/ipc/mock`).
- **The terminal UI:** its real screen, the `muster` binary in a real terminal over a throwaway daemon with stand-in agents.

Nothing of either app is redrawn. Record and render again when a design
changes.

It is in two parts:

- [`kit/`](./kit/README.md): what the videos are made from. The desktop app's recorder, which shoots the app and measures every crop and click point (`crops.js`); the terminal UI's, in [`kit/tui/`](./kit/tui/README.md); and the sound kit from Mixkit and its measurements (`AUDIO.md`, `beats.png`).
- [`launch/`](./launch/README.md): the videos. One page, `index.html`, whose `seek(t)` draws any moment of a cut (`?cut=tui` for the terminal UI's); each cut's times are in beats in its timeline (`timeline.js`, `timeline-tui.js`); `render.mjs` turns a cut into a video. Their rules are [`launch/CLAUDE.md`](./launch/CLAUDE.md) and their look [`launch/LOOK.md`](./launch/LOOK.md).

## Interface

`pnpm install` (and `pnpm install` in `apps/desktop`), then:

- `pnpm record`: builds the desktop app's mock (`pnpm build:mock` in `apps/desktop`), drives it through the story in headless Chromium, and writes `kit/shots` and `kit/crops.js`.
- `pnpm record:tui`: the terminal UI's shots and crops, in `kit/tui` (the `muster` on `PATH`).
- `pnpm audio`: fetches the sound kit into `kit/audio`.
- `pnpm check [tui]`: fails if any click or ring would land under a headline or off screen.
- `pnpm contact`: a frame per beat in `launch/out/contact.png`; `pnpm stills 9,17,45` single frames. Add `--cut tui` for the terminal UI's, in `launch/out/tui`.
- `pnpm render [--cut tui]`: the video, `launch/out/muster.mp4` or `launch/out/tui/muster.mp4` (1920x1080, 60 fps, motion blur, sound), then a scan for single-frame pops; `--sound` only remixes the sound onto the picture already rendered.
- `pnpm media [desktop|tui]`: the README's copies in `docs/media`: `muster.gif` and `muster.mp4`, or `muster-tui.gif` and `muster-tui.mp4` (800 px, 10 fps GIFs).

`ffmpeg` must be on `PATH`. Measuring the sound again (`kit/measure.py`) needs Python with numpy, and matplotlib for `beats.png`.

## Invariants

- What the videos say Muster does, it does: the windows and terminals are the apps' own, and the terminal's lines are the ones Muster prints. No invented numbers or names.
- The recorders reach no real daemon of the person's, tracker or agent: the desktop app's pretend world is changed only through `window.__muster`, as its end-to-end tests change it; the terminal UI's world is its own daemon in `/tmp/muster-demo`, removed afterwards. The machine shown is `you@203.0.113.7`, an address kept for documentation (`PRD-06`).
- Every frame is a pure function of time, and every time is in a cut's timeline.
- Generated files stay out of git (`kit/shots`, `kit/tui/shots`, `kit/audio/*.mp3`, `launch/out`); Mixkit's recordings are fetched, as its licenses do not allow passing them on. `docs/media` holds what the README shows.

## Side Effects

- `pnpm record` builds `apps/desktop/dist-mock`; `pnpm record:tui` creates and removes `/tmp/muster-demo` and runs a daemon there; the recorders and the renderer each serve files on a free port of `127.0.0.1` while they run.
- Chromium runs headless; the desktop recorder turns WebGL off so the app's terminal draws with its DOM renderer.
- `pnpm audio` downloads from `assets.mixkit.co`.

## Verification

- `pnpm check` and `pnpm check tui`, then `pnpm contact` (and `--cut tui`) and look at the sheets.
- Watch `launch/out/muster.mp4` and `launch/out/tui/muster.mp4` after rendering them.
