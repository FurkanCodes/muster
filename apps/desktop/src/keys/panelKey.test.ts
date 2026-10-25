// The panel key in the window (PRD §6a): it waits for one more key, which
// is read on the screen after it and handed to the panels shown; the
// arrows move and with Shift swap; the panel key again sends it on; Esc
// gives up; and xterm leaves both to the window.

import { afterEach, describe, expect, it } from "vitest";
import {
  answerPanelKeys,
  panelKeyWaiting,
  readKeymapFrom,
  takePanelKey,
  windowTakes,
} from "./panelKey";
import { keymapFor } from "./useKeymap";

const map = keymapFor({}, []);
const press = (chord: string) => new KeyboardEvent("keydown", { key: chord, cancelable: true });

afterEach(() => {
  answerPanelKeys(null);
  // Let go of a key left waiting.
  if (panelKeyWaiting()) takePanelKey(press("Escape"), "Escape");
});

describe("the panel key", () => {
  it("waits for the key after it, which the panels shown act on", () => {
    const heard: string[] = [];
    const waits: boolean[] = [];
    readKeymapFrom(
      () => map,
      (waiting) => waits.push(waiting),
    );
    answerPanelKeys((action) => heard.push(action));
    expect(windowTakes("⌃B")).toBe(true);
    expect(windowTakes("b")).toBe(false);
    expect(takePanelKey(press("⌃B"), "⌃B")).toBe(true);
    expect(panelKeyWaiting()).toBe(true);
    expect(windowTakes("v")).toBe(true);
    expect(takePanelKey(press("v"), "v")).toBe(true);
    expect(panelKeyWaiting()).toBe(false);
    for (const [chord] of [["⌃B"], ["ArrowLeft"], ["⌃B"], ["⇧ArrowDown"], ["⌃B"], ["⌃B"]]) {
      takePanelKey(press(chord as string), chord as string);
    }
    expect(heard).toEqual(["split_right", "focus_left", "swap_down", "send_panel_key"]);
    expect(waits).toEqual([true, false, true, false, true, false, true, false]);
  });

  it("gives up on Esc, and leaves other keys to the screens", () => {
    const heard: string[] = [];
    readKeymapFrom(
      () => map,
      () => {},
    );
    answerPanelKeys((action) => heard.push(action));
    expect(takePanelKey(press("j"), "j")).toBe(false);
    takePanelKey(press("⌃B"), "⌃B");
    expect(takePanelKey(press("Escape"), "Escape")).toBe(true);
    expect(heard).toEqual([]);
  });
});
