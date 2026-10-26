// The terminal UI's cut: every time in beats, at the same tempo as the
// desktop app's (timeline.js). Its shots and crops come from ../kit/tui,
// recorded from the real `muster` binary; camera positions are in the
// terminal's own pixels (its size is its shots'). Each key pressed shows
// under the headline with what the terminal UI's own hints call it.

export { at, BEAT, BPM, MUSIC_START } from "./timeline.js";

export const KIT = "kit/tui";
/** A terminal: no Mac buttons drawn over it. */
export const LIGHTS = false;
export const MUSIC = false;
export const LENGTH = 73;
export const FADE = [69, 73];
export const DROP = 0;

/** No opening act: Kep and the name, then the terminal. */
export const ACT1 = null;
export const REVEAL = { logo: 0, name: 0.5, tagline: 1, out: 4.4 };
export const TAGLINE = "Every agent, in your terminal.";
export const WINDOW_IN = 4.6;

export const HEADLINES = [
  { beat: 5, out: 15.5, place: "left", title: "Every agent in one list", sub: "Each in its own worktree; the one you pick streams beside the list.", accent: "one list", colour: "done" },
  { beat: 16, out: 29.5, place: "bottom", title: "A worktree, then an agent in it", sub: "Name the branch, give the first prompt; Muster starts the agent there.", accent: "worktree,", colour: "keys" },
  { beat: 30, out: 43.5, place: "right", title: "Know when one needs you", sub: "Its line turns, the top bar counts it, and the queue takes you straight to it.", accent: "needs you", colour: "waiting" },
  { beat: 44, out: 55.5, place: "left", title: "Undo any agent's turn", sub: "Every turn is kept. Put one back and the files return; the undo is kept too.", accent: "Undo", colour: "done" },
  { beat: 56, out: 65.5, place: "bottom", title: "From ticket to agent", sub: "A project's tickets, and an agent on one in a worktree of its own.", accent: "ticket", colour: "keys" },
];

/** The keys pressed, each with what the terminal UI's hints call it. */
export const KEYS = [
  { beat: 8, keys: ["↓"], hint: "move" },
  { beat: 16.2, keys: ["w"], hint: "worktree" },
  { beat: 19.5, keys: ["⏎"], hint: "confirm" },
  { beat: 21, keys: ["n"], hint: "session" },
  { beat: 25, keys: ["⏎"], hint: "start" },
  { beat: 33, keys: ["⇥"], hint: "next tab" },
  { beat: 35.5, keys: ["⏎"], hint: "type in it" },
  { beat: 37.5, keys: ["1"], hint: "Yes" },
  { beat: 44.2, keys: ["d"], hint: "changes" },
  { beat: 46.5, keys: ["⏎"], hint: "its diff" },
  { beat: 49, keys: ["z"], hint: "put back as before it" },
  { beat: 51, keys: ["y"], hint: "put the files back" },
  { beat: 56.2, keys: ["i"], hint: "tickets" },
  { beat: 58.5, keys: ["⏎"], hint: "start an agent" },
  { beat: 60.5, keys: ["⏎"], hint: "start" },
];

/** Which shot the terminal shows, from which beat. */
export const WINDOW_SHOTS = [
  [4.5, "home"],
  [8, "list"],
  [16.2, "worktree"],
  [18.6, "worktree-typed"],
  [19.5, "worktree-made"],
  [21, "new-session"],
  [24.2, "new-typed"],
  [25, "started"],
  [30, "asking"],
  [33, "queue"],
  [35.5, "answering"],
  [37.6, "answered"],
  [44.2, "changes"],
  [46.5, "turn-diff"],
  [49, "put-back"],
  [51, "put-back-done"],
  [56.2, "tickets"],
  [58.5, "assign"],
  [60.5, "assigned"],
];

/** The branch and the prompt, typed a letter at a time over the dialog before them. */
export const TYPED = [
  { crop: "worktree-text", from: 17, to: 18.4, until: 18.6, letters: 9, lead: 0 },
  { crop: "session-text", from: 22, to: 24, until: 24.2, letters: 27, lead: 0 },
];
export const SSH_ROWS = null;

/** The camera: one move at a time, `from` beat to `to` beat. */
export const CAMERA = [
  { from: 0, to: 0, x: 720, y: -720, zoom: 0.8, place: "centre" },
  { from: 4.6, to: 5.6, x: 720, y: 450, zoom: 0.8 },
  { from: 8.6, to: 9.6, on: "row-split", zoom: 1.3, dy: 120, left: true },
  { from: 15.6, to: 16.6, on: "worktree-dialog", zoom: 1.3, left: true },
  { from: 19.6, to: 20.4, on: "worktree-line", zoom: 1.5, dy: 60, left: true },
  { from: 20.6, to: 21.4, on: "session-dialog", zoom: 1.3, left: true },
  { from: 25.2, to: 26, on: "row-new", zoom: 1.5, dy: 40, left: true },
  { from: 29.6, to: 30.6, on: "row-asking", zoom: 1.3, left: true },
  { from: 33.2, to: 34, on: "question", zoom: 1.2, left: true },
  { from: 37.8, to: 38.6, on: "allowed", zoom: 1.2, left: true },
  { from: 43.6, to: 44.6, on: "changes", zoom: 1.1 },
  { from: 46.7, to: 47.4, on: "diff", zoom: 1.1 },
  { from: 49.1, to: 49.8, on: "put-back-dialog", zoom: 1.2 },
  { from: 51.2, to: 52, on: "changes-after", zoom: 1.2, top: true },
  { from: 55.6, to: 56.6, on: "ticket-list", zoom: 1.2, left: true, top: true },
  { from: 58.6, to: 59.3, on: "assign-dialog", zoom: 1.2, left: true },
  { from: 60.7, to: 61.5, on: "ticket-agent", zoom: 1.0, left: true },
  { from: 65.6, to: 66.6, x: 720, y: 450, zoom: 0.62, place: "centre" },
];

/** Rings round what the eye should find. */
export const RINGS = [
  { crop: "row-split", beat: 10, out: 15.2, colour: "working" },
  { crop: "row-flaky", beat: 11, out: 15.2, colour: "done" },
  { crop: "worktree-line", beat: 20.5, out: 20.9, colour: "keys" },
  { crop: "row-new", beat: 26.2, out: 29.2, colour: "keys" },
  { crop: "row-asking", beat: 31, out: 32.9, colour: "waiting" },
  { crop: "question", beat: 34.4, out: 37.4, colour: "waiting" },
  { crop: "allowed", beat: 38.8, out: 43.2, colour: "done" },
  { crop: "turn", beat: 45, out: 46.4, colour: "done" },
  { crop: "put-back-line", beat: 52.3, out: 55.2, colour: "done" },
  { crop: "ticket-line", beat: 57, out: 58.4, colour: "keys" },
];

export const CLICKS = [];

export const OUTRO = { logo: 66.5, install: 67.5, url: 68.2 };

/** The sound: typing where text is typed, a soft click on each key. */
export const SOUNDS = [
  ...KEYS.map((key) => ({ sound: "click", beat: key.beat, gain: -8 })),
  ...TYPED.map((typing) => ({ sound: "typing", beat: typing.from, gain: -8, slice: { start: 1.0 + typing.from / 10, beats: typing.to - typing.from } })),
];
