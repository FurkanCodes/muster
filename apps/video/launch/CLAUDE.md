# Launch video rules

The cuts in this folder (the desktop app's and the terminal UI's) follow
these rules; [LOOK.md](LOOK.md) is how they look, and
[`../kit/AUDIO.md`](../kit/AUDIO.md) their tempo and sounds.

## Product

- Every product pixel comes from a kit's shots: real screenshots of the desktop app (`../kit/shots`) or of the terminal UI in a real terminal (`../kit/tui/shots`), and crops of them, at the positions in the kit's `crops.js`. Never redraw UI.
- Need a state the shots don't have? Change the story in the kit's recorder (`../kit/record.mjs`, through the pretend daemon's `window.__muster`; `../kit/tui/record.mjs`, through key presses and the world's hooks) and record it.
- No invented numbers, names or prices: only what is on the shots, or in Muster's own output (`crates/muster/src`) and README.
- `../kit` is shared and read-only from here. Everything the clip makes is written into this folder.

## Engine

- One `index.html` with `window.seek(t)`. Every frame is a pure function of time: no CSS animations, transitions, timers or `requestAnimationFrame`, and nothing remembered between frames.
- Every time lives in the cut's timeline (`timeline.js`, `timeline-tui.js`), written in beats. The picture and the sound both read it.
- Springs are closed-form (a formula of time) with a small overshoot. A value that changes target more than once is a sum of springs, one per change.
- The camera is one transform on one container. Eased keyframes, one move at a time, zoom interpolated in log space.

## Render

- 1920x1080, 60 fps. Capture 4 subframes per frame and blend them with ffmpeg `tmix` for motion blur; 8 where something moves more than 15 px a frame. H.264, yuv420p, AAC.

## Sound

- Music and effects come from `../kit/audio` (real recordings, listed in `../kit/AUDIO.md`). Never synthesize.
- The music starts on a downbeat. Each effect is placed so its measured peak lands on its event. Loudness -14 LUFS.
- These cuts have no music (`MUSIC = false` in their timelines, asked for on 2026-10-26): the effects keep the level they have over it, and the music's beat grid still sets every time. The sound stays simple: typing where text is typed, a soft click where something is clicked or a key pressed.

## Before showing it

- Render one frame per beat into `out/contact.png`, look at it, and fix the three worst problems.
- Scan the finished video for single-frame pops (a frame that jumps far more than its neighbours) and fix them.
- Say what was fixed and what would still change.
