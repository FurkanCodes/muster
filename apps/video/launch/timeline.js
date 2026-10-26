// Every time in the clip, in beats. The picture (scenes.js) and the sound
// (render.mjs) both read it. Tempo and grid: ../kit/AUDIO.md.
//
// Points and crops are named from ../kit/crops.js; camera positions are in
// the window's own pixels (1440 by 900).

/** Rising Forest, measured: 123.97 BPM, a beat every 0.48399 s. */
export const BPM = 123.97;
export const BEAT = 0.48399;
/** The track's time at the clip's beat 0: its beat 79, a downbeat, 16 beats before its drop (46.153 s). */
export const MUSIC_START = 38.409;
/**
 * Whether the music plays. Off: the effects alone, each as loud as it is
 * over the music, on the music's beat grid all the same.
 */
export const MUSIC = false;
export const DROP = 16;
/** The clip's length, and where the music fades. */
export const LENGTH = 92;
export const FADE = [88, 92];

/** Seconds at a beat. */
export const at = (beat) => beat * BEAT;

// Act 1: the problem, on the quiet build before the drop.

export const ACT1 = {
  /** "Your agents are working.", a word a beat; then it slides up as the second line rises. */
  line1: { words: ["Your", "agents", "are", "working."], beats: [0, 1, 2, 3], accent: "working.", colour: "working" },
  up: 4,
  line2: { text: "One of them is waiting on you.", beat: 4, accent: "waiting", colour: "waiting" },
  /** Real rows from the list stack in under the lines, a beat each. */
  cards: [
    { crop: "row-split", beat: 6 },
    { crop: "row-flaky", beat: 7 },
    { crop: "row-agp", beat: 8 },
    { crop: "row-gemini", beat: 9 },
  ],
  /** The waiting row wears its real "Needs you" badge. */
  badge: { crop: "badge-gemini", beat: 9.5 },
  /** The stack squeezes into the one dot that matters. */
  squeeze: 12,
  dot: 12.9,
  question: { text: "Which one?", beat: 13.4, out: 15.6 },
  pulses: [14, 15],
};

// The drop: Muster.

export const REVEAL = { logo: DROP, name: DROP + 0.5, tagline: DROP + 1, out: 19.5 };

// The features, after the drop: a headline each, the real window, and the
// camera finding what the headline says. Each headline takes a side of the
// screen (`place`); the window moves to the space it leaves.

export const HEADLINES = [
  { beat: 20, out: 27.5, place: "right", title: "Every agent in one list", sub: "Claude Code, Codex, Gemini CLI and more: what each is doing, and for how long.", accent: "one list", colour: "done" },
  { beat: 28, out: 39.5, place: "bottom", title: "Each in its own worktree", sub: "Say what to do; Muster makes the branch and starts the agent there.", accent: "own worktree", colour: "keys" },
  { beat: 40, out: 51.5, place: "left", title: "Know when one needs you", sub: "Its line turns, Needs you counts it, and one click takes you there.", accent: "needs you", colour: "waiting" },
  { beat: 52, out: 63.5, place: "top", title: "Undo any agent's turn", sub: "Muster keeps every turn. Put one back and the files return; the undo is kept too.", accent: "Undo", colour: "done" },
  { beat: 64, out: 75.5, place: "left", title: "From ticket to pull request", sub: "Hand a GitHub, GitLab, Jira or Linear ticket to an agent.", accent: "pull request", colour: "done" },
  { beat: 76, out: 83.5, place: "right", title: "On your own server, too", sub: "muster ssh sets it up on any machine you can ssh into.", accent: "server", colour: "keys" },
];

/** Which shot the window shows, from which beat. */
export const WINDOW_SHOTS = [
  [20, "list"],
  [28, "start-empty"],
  [32.25, "started"],
  [41, "asking"],
  [44.25, "question"],
  [47, "answered"],
  [49, "finished"],
  [52, "turns"],
  [55.25, "turn-diff"],
  [58.25, "put-back"],
  [60.25, "put-back-done"],
  [64, "tickets"],
  [67.25, "assign"],
  [69.25, "assigned"],
  [72, "project"],
  [76, "ssh"],
];

/** The prompt typed into the start box: the filled shot's prompt, uncovered left to right. */
export const TYPED = { crop: "prompt", from: 29, to: 31, until: 32.25, width: 236 };

/** `muster ssh` in Muster's terminal: its five rows, shown as they are printed. */
export const SSH_ROWS = { crop: "terminal-ssh", rows: 5, beats: [77, 78.5, 79, 79.5, 80.5], typed: [0, 4] };

/**
 * The camera: one move at a time, `from` beat to `to` beat, to a point in
 * the window and a zoom. A crop's name aims at its middle; `left` keeps
 * the window's left edge, and the list, in view; `top` its title line.
 */
export const WIDE = { x: 720, y: 450, zoom: 0.86 };
export const CAMERA = [
  { from: 0, to: 0, x: 720, y: -720, zoom: 0.86, place: "centre" },
  { from: 19.6, to: 20.6, x: 720, y: 450, zoom: 0.78 },
  { from: 21.4, to: 22.4, on: "list-working", zoom: 1.35, dy: 120, left: true },
  { from: 26.8, to: 28, on: "start-box", zoom: 1.4, left: true },
  { from: 32.5, to: 33.4, on: "row-new", zoom: 1.8 },
  { from: 39.4, to: 40.4, on: "row-codex-asking", zoom: 1.6, dy: -170 },
  { from: 44.4, to: 45.3, on: "badge-codex", zoom: 1.3, dy: 160, left: true },
  { from: 48.4, to: 49.3, on: "row-codex-finished", zoom: 1.6 },
  { from: 51.6, to: 52.6, x: 640, y: 300, zoom: 1.2 },
  { from: 55.3, to: 56.2, on: "diff", zoom: 1.25, top: true },
  { from: 58.3, to: 59, on: "dialog-put-back", zoom: 1.5 },
  { from: 60.4, to: 61.3, on: "put-back-line", zoom: 1.45, dx: 160, dy: -40 },
  { from: 63.6, to: 64.6, x: 900, y: 450, zoom: 0.95 },
  { from: 67.3, to: 68, on: "dialog-assign", zoom: 1.35 },
  { from: 69.4, to: 70.3, on: "row-ticket", zoom: 1.8 },
  { from: 71.7, to: 72.7, on: "pr-approved", zoom: 1.5, dy: 40 },
  { from: 75.6, to: 76.6, on: "terminal-ssh", zoom: 1.7, left: true },
  { from: 83.6, to: 84.6, x: 720, y: 450, zoom: 0.7, place: "centre" },
];

/** Rings round what the eye should find, in the colour of what it means. */
export const RINGS = [
  { crop: "list-working", beat: 23, out: 27.2, colour: "working" },
  { crop: "list-finished", beat: 24, out: 27.2, colour: "done" },
  { crop: "list-needs-you", beat: 25, out: 27.2, colour: "waiting" },
  { crop: "branch-new", beat: 34, out: 38.5, colour: "keys" },
  { crop: "row-codex-asking", beat: 41.5, out: 43.6, colour: "waiting" },
  { crop: "needs-you-tab", beat: 42.2, out: 43.6, colour: "waiting" },
  { crop: "badge-codex", beat: 45.5, out: 46.8, colour: "waiting" },
  { crop: "row-codex-finished", beat: 49.5, out: 51.4, colour: "done" },
  { crop: "overlap", beat: 53.4, out: 54.8, colour: "working" },
  { crop: "put-back-line", beat: 61.2, out: 63.4, colour: "done" },
  { crop: "header-after", beat: 62, out: 63.4, colour: "done" },
  { crop: "ticket-line", beat: 65, out: 66.8, colour: "keys" },
  { crop: "branch-ticket", beat: 70.6, out: 71.6, colour: "keys" },
  { crop: "pr-approved", beat: 73, out: 75.4, colour: "done" },
];

/** The pointer's clicks: on a point measured on the shot shown, at a beat. */
export const CLICKS = [
  { point: "start", beat: 32 },
  { point: "codex", beat: 44 },
  { point: "turn", beat: 55 },
  { point: "put-back", beat: 58 },
  { point: "put-back-files", beat: 60 },
  { point: "assign", beat: 67 },
  { point: "start-assigned", beat: 69 },
];

/** The ending: the window steps back, Muster's name and how to get it. */
export const OUTRO = { logo: 84.5, install: 85.5, url: 86.2 };

/**
 * The sound, kept simple: typing where something is typed, and a soft
 * click where the pointer clicks or a row lands. Each effect's measured
 * peak lands on its beat. `gain` in dB; `slice` plays part of a long
 * recording, from `start` for `beats`.
 */
export const SOUNDS = [
  ...ACT1.cards.map((card) => ({ sound: "click", beat: card.beat, gain: -9 })),
  { sound: "typing", beat: TYPED.from, gain: -8, slice: { start: 1.0, beats: TYPED.to - TYPED.from } },
  ...CLICKS.map((click) => ({ sound: "click", beat: click.beat, gain: -7 })),
  { sound: "click", beat: 47, gain: -9 },
  { sound: "typing", beat: SSH_ROWS.beats[0], gain: -9, slice: { start: 3.0, beats: 1 } },
  { sound: "typing", beat: SSH_ROWS.beats[4], gain: -9, slice: { start: 6.0, beats: 1.2 } },
];
