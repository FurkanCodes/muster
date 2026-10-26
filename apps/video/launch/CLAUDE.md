# Launch video rules

The clip in this folder follows these rules; [LOOK.md](LOOK.md) is how it
looks, and [`../kit/AUDIO.md`](../kit/AUDIO.md) its tempo and sounds.

## Product

- Every product pixel comes from `../kit/shots`: real screenshots of the desktop app and crops of them, at the positions in `../kit/crops.js`. Never redraw UI.
- Need a state the shots don't have? Change the real app in `../kit/record.mjs` (the pretend daemon, through `window.__muster`) and record it.
- No invented numbers, names or prices: only what is on the shots, or in Muster's own output (`crates/muster/src`) and README.
- `../kit` is shared and read-only from here. Everything the clip makes is written into this folder.

## Engine

- One `index.html` with `window.seek(t)`. Every frame is a pure function of time: no CSS animations, transitions, timers or `requestAnimationFrame`, and nothing remembered between frames.
- Every time lives in `timeline.js`, written in beats. The picture and the sound both read it.
- Springs are closed-form (a formula of time) with a small overshoot. A value that changes target more than once is a sum of springs, one per change.
- The camera is one transform on one container. Eased keyframes, one move at a time, zoom interpolated in log space.

## Render

- 1920x1080, 60 fps. Capture 4 subframes per frame and blend them with ffmpeg `tmix` for motion blur; 8 where something moves more than 15 px a frame. H.264, yuv420p, AAC.

## Sound

- Music and effects come from `../kit/audio` (real recordings, listed in `../kit/AUDIO.md`). Never synthesize.
- The music starts on a downbeat. Each effect is placed so its measured peak lands on its event. Loudness -14 LUFS.
- This cut has no music (`MUSIC = false` in `timeline.js`, asked for on 2026-10-26): the effects keep the level they have over it, and the music's beat grid still sets every time.

## Before showing it

- Render one frame per beat into `out/contact.png`, look at it, and fix the three worst problems.
- Scan the finished video for single-frame pops (a frame that jumps far more than its neighbours) and fix them.
- Say what was fixed and what would still change.
