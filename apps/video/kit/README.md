# kit

## Overview

What the cuts in [`../launch`](../launch/README.md) are made from, and read
only from there: the desktop app's shots, where each crop and click is on
them, and the sound. The terminal UI's shots and crops are in
[`tui/`](./tui/README.md).

## Interface

- `record.mjs` (`pnpm record`): builds the desktop app's mock, drives its window through the story in headless Chromium at 2x, and writes:
  - `shots/*.png`: one screenshot a step, and `kep.png`, the app's icon;
  - `crops.js`: `shots` (file and size), `crops` (a box on a shot, by name: rows, banners, badges, dialogs, the terminal's lines) and `points` (where each click lands), all in the window's pixels, measured from the page.
- `fetch-audio.sh` (`pnpm audio`): fetches the music and effects into `audio/`, by the names the clip uses.
- `measure.py`: measures them with numpy: a track's tempo, beat grid, first downbeat and drop (`python3 measure.py music FILE --plot beats.png`), an effect's length and peak (`python3 measure.py effects FILE…`). Prints JSON.
- `AUDIO.md`: each recording's source page, license, length and peak, and the music's tempo, beat grid and drop. `beats.png`: the music's waveform with every beat, bar and the drop marked.

## Invariants

- A crop or point is always measured from the page, never typed in: change the story in `record.mjs` and record again.
- Each crop's name is unique across shots.
- The recorder changes the pretend world only through `window.__muster`.

## Side Effects

- Writes `shots/` and `crops.js`; builds `apps/desktop/dist-mock`; serves it on a free local port while recording.
- `fetch-audio.sh` downloads from `assets.mixkit.co`; the files stay out of git.

## Verification

- After `pnpm record`, look at the crops: `pnpm contact` in the clip shows each in place.

## Subdirectories

- tui: [./tui/README.md](./tui/README.md): the terminal UI's recorder: the real `muster` binary in a real terminal over a throwaway daemon, its shots and crops.
