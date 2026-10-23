// A key as Muster's settings write it (`[keys]`, `muster_core::keys::Chord`):
// a character, or a key with a name, with Ctrl or Alt; a capital letter is
// the letter with Shift. Read and written here exactly as the core reads
// and writes it, so a key set in this window reads the same in the
// terminal UI. ⌘ is never in one: ⌘ keys are this window's own.

/** A key by itself: one character (`a`, `R`, `?`, ` `), or a named key. */
export type Key =
  | { char: string }
  | {
      named:
        | "tab"
        | "backtab"
        | "esc"
        | "backspace"
        | "enter"
        | "up"
        | "down"
        | "left"
        | "right"
        | "pageup"
        | "pagedown"
        | "home"
        | "end";
    }
  | { f: number };

export interface Chord {
  key: Key;
  ctrl: boolean;
  alt: boolean;
}

type Named = Extract<Key, { named: string }>["named"];

const NAMED: Readonly<Record<string, Named>> = {
  tab: "tab",
  backtab: "backtab",
  esc: "esc",
  escape: "esc",
  backspace: "backspace",
  enter: "enter",
  return: "enter",
  up: "up",
  down: "down",
  left: "left",
  right: "right",
  pageup: "pageup",
  pagedown: "pagedown",
  home: "home",
  end: "end",
};

function isControl(char: string): boolean {
  const code = char.codePointAt(0) ?? 0;
  return code < 0x20 || (code >= 0x7f && code < 0xa0);
}

/** `char` with an ASCII letter made a capital, as Rust's `to_ascii_uppercase`. */
function asciiUpper(char: string): string {
  return /^[a-z]$/.test(char) ? char.toUpperCase() : char;
}

/** The chord `text` writes (`a`, `R`, `ctrl-]`, `shift-tab`, `f5`); `null` where it is no key. */
export function parseChord(text: string): Chord | null {
  let ctrl = false;
  let alt = false;
  let shift = false;
  let rest = text.trim();
  for (;;) {
    const lower = rest.toLowerCase();
    const prefix = ["ctrl-", "alt-", "shift-"].find(
      (candidate) => lower.startsWith(candidate) && lower.length > candidate.length,
    );
    if (!prefix) break;
    if (prefix === "ctrl-") ctrl = true;
    else if (prefix === "alt-") alt = true;
    else shift = true;
    rest = rest.slice(prefix.length);
  }
  const chars = Array.from(rest);
  if (chars.length === 1 && !isControl(chars[0] as string)) {
    const char = chars[0] as string;
    return { key: { char: shift ? asciiUpper(char) : char }, ctrl, alt };
  }
  const name = rest.toLowerCase();
  if (name === "space") return { key: { char: " " }, ctrl, alt };
  const named = NAMED[name];
  if (named) return { key: { named: named === "tab" && shift ? "backtab" : named }, ctrl, alt };
  const f = /^f(\d{1,2})$/.exec(name);
  if (f) {
    const n = Number(f[1]);
    if (n >= 1 && n <= 24) return { key: { f: n }, ctrl, alt };
  }
  return null;
}

/** As `[keys]` writes it. */
export function writtenChord(chord: Chord): string {
  const mods = `${chord.ctrl ? "ctrl-" : ""}${chord.alt ? "alt-" : ""}`;
  const key = chord.key;
  if ("char" in key) return mods + (key.char === " " ? "space" : key.char);
  if ("f" in key) return `${mods}f${key.f}`;
  return mods + (key.named === "backtab" ? "shift-tab" : key.named);
}

export function sameChord(a: Chord, b: Chord): boolean {
  return writtenChord(a) === writtenChord(b);
}

const LABELS: Readonly<Record<Named, string>> = {
  tab: "⇥",
  backtab: "⇧⇥",
  esc: "⎋",
  backspace: "⌫",
  enter: "⏎",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  pageup: "PgUp",
  pagedown: "PgDn",
  home: "Home",
  end: "End",
};

/** As the person reads it on a key cap, in the Mac's symbols: `d`, `⇧R`, `⌃]`, `⏎`. */
export function chordLabel(chord: Chord): string {
  const mods = `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}`;
  const key = chord.key;
  if ("f" in key) return `${mods}F${key.f}`;
  if ("named" in key) return mods + LABELS[key.named];
  if (key.char === " ") return `${mods}␣`;
  if (/^[A-Z]$/.test(key.char)) return `${mods}⇧${key.char}`;
  return mods + (mods ? key.char.toUpperCase() : key.char);
}

/** Whether a session's program could never take it as typed: with Ctrl or Alt, or a capital. */
export function untyped(chord: Chord): boolean {
  return chord.ctrl || chord.alt || ("char" in chord.key && /^[A-Z]$/.test(chord.key.char));
}

/** Whether a terminal tab (`muster attach`) can tell it from typing: a lower-case letter with ⌃ or ⌥ (`Chord::in_tab`). */
export function inTab(chord: Chord): boolean {
  return (chord.ctrl || chord.alt) && "char" in chord.key && /^[a-z]$/.test(chord.key.char);
}

const DOM_NAMES: Readonly<Record<Named, string>> = {
  tab: "Tab",
  backtab: "⇧Tab",
  esc: "Escape",
  backspace: "Backspace",
  enter: "Enter",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
  pageup: "PageUp",
  pagedown: "PageDown",
  home: "Home",
  end: "End",
};

/** The key as the window's listener names a press of it (`chordOf` in app/keys.ts). */
export function domChord(chord: Chord): string {
  const mods = `${chord.ctrl ? "⌃" : ""}${chord.alt ? "⌥" : ""}`;
  const key = chord.key;
  if ("f" in key) return `${mods}F${key.f}`;
  if ("named" in key) return mods + DOM_NAMES[key.named];
  if (!mods) return key.char;
  return /^[A-Z]$/.test(key.char) ? `${mods}⇧${key.char}` : mods + key.char.toUpperCase();
}

const CODES: Readonly<Record<string, string>> = {
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Slash: "/",
  Minus: "-",
  Equal: "=",
  Comma: ",",
  Period: ".",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Space: " ",
};

const EVENT_NAMES: Readonly<Record<string, Named>> = {
  Escape: "esc",
  Backspace: "backspace",
  Enter: "enter",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  PageUp: "pageup",
  PageDown: "pagedown",
  Home: "home",
  End: "end",
};

type KeyPress = Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/**
 * The character on the key at `code`, a capital with `shift`: what a press
 * with Ctrl or Alt is named by, as the Mac's Option composes another
 * character (⌥X types ≈).
 */
export function charOfCode(code: string, shift: boolean): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return shift ? (letter[1] as string) : (letter[1] as string).toLowerCase();
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1] as string;
  return CODES[code] ?? null;
}

/**
 * The chord a press is, as `[keys]` would write it (the terminal UI's
 * `keys::chord`); `null` for a modifier alone, a key Muster has no name
 * for, or one with ⌘, which is this window's own.
 */
export function pressedChord(press: KeyPress): Chord | null {
  if (press.metaKey) return null;
  const ctrl = press.ctrlKey;
  const alt = press.altKey;
  if (press.key === "Tab") return { key: { named: press.shiftKey ? "backtab" : "tab" }, ctrl, alt };
  const named = EVENT_NAMES[press.key];
  if (named) return { key: { named }, ctrl, alt };
  const f = /^F(\d{1,2})$/.exec(press.key);
  if (f) return { key: { f: Number(f[1]) }, ctrl, alt };
  if (ctrl || alt) {
    const char = charOfCode(press.code, press.shiftKey);
    return char === null ? null : { key: { char }, ctrl, alt };
  }
  const chars = Array.from(press.key);
  if (chars.length !== 1 || isControl(press.key)) return null;
  return { key: { char: press.key }, ctrl, alt };
}
