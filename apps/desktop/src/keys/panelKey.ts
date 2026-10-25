// The panel key in the window (`PRODUCT.md` §6a, the terminal UI's
// `app::prefix`): pressed anywhere, a terminal's keys included, it waits for
// one more key, read on the table's screen after it (`prefix`), which the
// panels shown act on (`usePrefixKeys`). The panel key again, or its own
// key after it, sends it on to the terminal with the keys. Esc, or a key
// that does nothing there, gives up. Nothing of either reaches a program.

import { domChord } from "./chord";
import { chordsOf, type Keymap, labelOf, screenKeys } from "./keymap";

/** What the panels shown do with the key after the panel key. */
export type PanelKeyHandler = (action: string, chord: string) => void;

let keymapNow: (() => Keymap) | null = null;
let handler: PanelKeyHandler | null = null;
let waiting = false;
let told: ((waiting: boolean) => void) | null = null;

/** Where the person's keymap is read from now, set as the window starts. */
export function readKeymapFrom(read: () => Keymap, onWaiting: (waiting: boolean) => void): void {
  keymapNow = read;
  told = onWaiting;
}

/** Lets go of a panel key waiting and of where the keymap was read, as the window's keys are taken away. */
export function stopPanelKeys(): void {
  waiting = false;
  keymapNow = null;
  told = null;
}

/** The panels shown, which the key after the panel key acts on; null as they go. */
export function answerPanelKeys(next: PanelKeyHandler | null): void {
  handler = next;
}

/** Whether the panel key waits for the key after it. */
export function panelKeyWaiting(): boolean {
  return waiting;
}

/** Whether `event` is the panel key, as the person set it. */
function isPanelKey(map: Keymap, chord: string): boolean {
  return chordsOf(map, "panel_key").some((one) => domChord(one) === chord);
}

/** The arrows after the panel key move the keys, and with Shift swap, whatever is set. */
const ARROWS: Readonly<Record<string, string>> = {
  ArrowLeft: "focus_left",
  ArrowRight: "focus_right",
  ArrowUp: "focus_up",
  ArrowDown: "focus_down",
  "⇧ArrowLeft": "swap_left",
  "⇧ArrowRight": "swap_right",
  "⇧ArrowUp": "swap_up",
  "⇧ArrowDown": "swap_down",
};

/** Whether xterm leaves a key down, `chord` as the window names it, to the window: the panel key, or the key after it. */
export function windowTakes(chord: string): boolean {
  if (!keymapNow) return false;
  return waiting || isPanelKey(keymapNow(), chord);
}

/**
 * The window's keys, first: the panel key starts waiting, and the key after
 * it is acted on. Whether `event`, `chord` as the window names it, was
 * either.
 */
export function takePanelKey(event: KeyboardEvent, chord: string): boolean {
  if (!keymapNow) return false;
  const map = keymapNow();
  if (waiting) {
    waiting = false;
    told?.(false);
    event.preventDefault();
    if (chord === "Escape") return true;
    const action = isPanelKey(map, chord)
      ? "send_panel_key"
      : (ARROWS[chord] ?? screenKeys(map, "prefix").get(chord));
    if (action) handler?.(action, chord);
    return true;
  }
  if (!isPanelKey(map, chord)) return false;
  event.preventDefault();
  waiting = true;
  told?.(true);
  return true;
}

/** What the status bar says while the panel key waits: the keys after it, the most used first. */
export function waitingHint(map: Keymap): string {
  const after = (action: string, what: string) => {
    const key = labelOf(map, action);
    return key === "none" ? null : `${key} ${what}`;
  };
  return [
    `${labelOf(map, "panel_key")}:`,
    after("split_right", "split →"),
    after("split_down", "split ↓"),
    "←↓↑→ move",
    "⇧←↓↑→ swap",
    after("zoom", "zoom"),
    after("take_off", "off the screen"),
    after("close_panel", "close"),
    "1-9 a panel",
    after("panel_keys", "keys"),
    "esc give up",
  ]
    .filter((part): part is string => part !== null)
    .join("  ");
}
