import { describe, expect, it } from "vitest";
import { chordOf } from "../app/keys";
import { chordLabel, domChord, parseChord, pressedChord, untyped, writtenChord } from "./chord";
import { ACTIONS } from "./keymap";

function chord(text: string) {
  const parsed = parseChord(text);
  if (!parsed) throw new Error(`${text} is no key`);
  return parsed;
}

describe("a key as [keys] writes it", () => {
  it("reads and writes as the core does", () => {
    for (const text of [
      "a",
      "R",
      "?",
      "ctrl-]",
      "alt-x",
      "ctrl-\\",
      "tab",
      "shift-tab",
      "esc",
      "backspace",
      "space",
      "f5",
      "1",
    ]) {
      expect(writtenChord(chord(text))).toBe(text);
    }
    expect(chord("shift-q")).toEqual(chord("Q"));
    expect(chord("Ctrl-]")).toEqual(chord("ctrl-]"));
    expect(writtenChord(chord("escape"))).toBe("esc");
    expect(writtenChord(chord("return"))).toBe("enter");
  });

  it("refuses what is no key", () => {
    for (const not of ["", "ctrl-", "abc", "f99", "f0", "\u0007"]) {
      expect(parseChord(not)).toBeNull();
    }
  });

  it("is labelled in the Mac's symbols, a capital with ⇧", () => {
    expect(chordLabel(chord("d"))).toBe("d");
    expect(chordLabel(chord("R"))).toBe("⇧R");
    expect(chordLabel(chord("Q"))).toBe("⇧Q");
    expect(chordLabel(chord("ctrl-]"))).toBe("⌃]");
    expect(chordLabel(chord("alt-x"))).toBe("⌥X");
    expect(chordLabel(chord("tab"))).toBe("⇥");
    expect(chordLabel(chord("shift-tab"))).toBe("⇧⇥");
    expect(chordLabel(chord("esc"))).toBe("⎋");
    expect(chordLabel(chord("backspace"))).toBe("⌫");
    expect(chordLabel(chord("space"))).toBe("␣");
    expect(chordLabel(chord("f5"))).toBe("F5");
  });

  it("says which keys a program could never take as typed", () => {
    expect(untyped(chord("Q"))).toBe(true);
    expect(untyped(chord("ctrl-]"))).toBe(true);
    expect(untyped(chord("alt-q"))).toBe(true);
    expect(untyped(chord("x"))).toBe(false);
    expect(untyped(chord("?"))).toBe(false);
  });
});

/** A press of `chord` on a Mac's US keyboard, as WebKit reports it. */
function press(key: string, code: string, mods: Partial<KeyboardEvent> = {}) {
  return { key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods };
}

describe("a press", () => {
  it("is the chord [keys] would write, Option's composed character set aside", () => {
    expect(pressedChord(press("d", "KeyD"))).toEqual(chord("d"));
    expect(pressedChord(press("R", "KeyR", { shiftKey: true }))).toEqual(chord("R"));
    expect(pressedChord(press("?", "Slash", { shiftKey: true }))).toEqual(chord("?"));
    expect(pressedChord(press("≈", "KeyX", { altKey: true }))).toEqual(chord("alt-x"));
    expect(pressedChord(press("]", "BracketRight", { ctrlKey: true }))).toEqual(chord("ctrl-]"));
    expect(pressedChord(press("Tab", "Tab", { shiftKey: true }))).toEqual(chord("shift-tab"));
    expect(pressedChord(press(" ", "Space"))).toEqual(chord("space"));
    expect(pressedChord(press("F5", "F5"))).toEqual(chord("f5"));
    expect(pressedChord(press("Escape", "Escape"))).toEqual(chord("esc"));
  });

  it("with ⌘, or of a modifier alone, is none", () => {
    expect(pressedChord(press("k", "KeyK", { metaKey: true }))).toBeNull();
    expect(pressedChord(press("Shift", "ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(pressedChord(press("Dead", "KeyE", {}))).toBeNull();
  });

  it("is named by the window's listener as the keymap names its chord", () => {
    const presses: [string, ReturnType<typeof press>][] = [
      ["d", press("d", "KeyD")],
      ["T", press("T", "KeyT", { shiftKey: true })],
      ["?", press("?", "Slash", { shiftKey: true })],
      ["1", press("1", "Digit1")],
      ["ctrl-\\", press("\\", "Backslash", { ctrlKey: true })],
      ["ctrl-]", press("]", "BracketRight", { ctrlKey: true })],
      ["alt-x", press("≈", "KeyX", { altKey: true })],
      ["ctrl-X", press("X", "KeyX", { ctrlKey: true, shiftKey: true })],
      ["tab", press("Tab", "Tab")],
      ["shift-tab", press("Tab", "Tab", { shiftKey: true })],
      ["esc", press("Escape", "Escape")],
      ["backspace", press("Backspace", "Backspace")],
      ["space", press(" ", "Space")],
      ["f5", press("F5", "F5")],
    ];
    for (const [written, pressed] of presses) {
      expect(chordOf(pressed), written).toBe(domChord(chord(written)));
    }
  });

  it("names every key of Muster's own table", () => {
    for (const action of ACTIONS) {
      for (const key of action.keys)
        expect(parseChord(key), `${action.name} ${key}`).not.toBeNull();
    }
  });
});
