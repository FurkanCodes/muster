# video

## Overview

The short video at the top of the repository's README: Muster's desktop
app, its real window, cut on a beat grid. Every pixel of the app in it is a
screenshot of the app running over its pretend daemon
(`apps/desktop/src/ipc/mock`); nothing of the app is redrawn. Re-record and
render again when the app's design changes.

It is in two parts:

- [`kit/`](./kit/README.md): what the clip is made from. The recorder that shoots the app and measures every crop and click point (`crops.js`), the sound kit from Mixkit and its measurements (`AUDIO.md`, `beats.png`).
- [`launch/`](./launch/README.md): the clip. One page, `index.html`, whose `seek(t)` draws any moment; `timeline.js` holds every time, in beats; `render.mjs` turns it into the video. Its rules are [`launch/CLAUDE.md`](./launch/CLAUDE.md) and its look [`launch/LOOK.md`](./launch/LOOK.md).

## Interface

`pnpm install`, then, in order:

- `pnpm record`: builds the desktop app's mock (`pnpm build:mock` in `apps/desktop`), drives it through the story in headless Chromium, and writes `kit/shots` and `kit/crops.js`.
- `pnpm audio`: fetches the sound kit into `kit/audio`.
- `pnpm check`: fails if any click or ring would land under a headline or off screen.
- `pnpm contact`: a frame per beat in `launch/out/contact.png`; `pnpm stills 9,17,45` single frames.
- `pnpm render`: the clip, `launch/out/muster.mp4` (1920x1080, 60 fps, motion blur, sound), then a scan for single-frame pops; `pnpm sound` only remixes the sound onto the picture already rendered.
- `pnpm media`: the README's copies in `docs/media`: `muster.gif` (800 px, 10 fps) and `muster.mp4`.

`ffmpeg` must be on `PATH`. Measuring the sound again (`kit/measure.py`) needs Python with numpy, and matplotlib for `beats.png`.

## Invariants

- What the clip says Muster does, it does: the window is the app's own, and the terminal's lines are the ones `crates/muster/src/ssh.rs` prints. No invented numbers or names.
- The recorder reaches no real daemon, tracker or agent; the pretend world is changed only through `window.__muster`, as the desktop app's end-to-end tests change it. The machine shown is `you@203.0.113.7`, an address kept for documentation (`PRD-06`).
- Every frame is a pure function of time, and every time is in `launch/timeline.js`.
- Generated files stay out of git (`kit/shots`, `kit/audio/*.mp3`, `launch/out`); Mixkit's recordings are fetched, as its licenses do not allow passing them on. `docs/media` holds what the README shows.

## Side Effects

- `pnpm record` builds `apps/desktop/dist-mock`; the recorder and the renderer each serve files on a free port of `127.0.0.1` while they run.
- Chromium runs headless; the recorder turns WebGL off so the terminal draws with its DOM renderer.
- `pnpm audio` downloads from `assets.mixkit.co`.

## Verification

- `pnpm check`, then `pnpm contact` and look at the sheet.
- Watch `launch/out/muster.mp4` after `pnpm render`.
