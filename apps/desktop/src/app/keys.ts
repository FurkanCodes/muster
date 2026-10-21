// Keys: Muster is driven from the keyboard first (README: Keys). One
// listener on the window for the whole app; screens add their keys while
// they are shown. While a terminal has the keys, every key goes to its
// program but the one that leaves it (⇧Q) and ⌘ shortcuts.

import {
  createContext,
  createElement,
  type ReactNode,
  use,
  useEffect,
  useRef,
  useState,
} from "react";
import { charOfCode } from "../keys/chord";
import type { App } from "./context";

/** A key as Muster names it: `d`, `T`, `?`, `Enter`, `⌘K`, `⌥X`, `⇧Tab`. */
export function chordOf(
  event: Pick<KeyboardEvent, "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
): string {
  const mods = `${event.metaKey ? "⌘" : ""}${event.ctrlKey ? "⌃" : ""}${event.altKey ? "⌥" : ""}`;
  // With ⌃ or ⌥, a character is named by the key it is on (⌥X, not ⌥≈).
  const at = (event.ctrlKey || event.altKey) && !event.metaKey;
  let key = (at && charOfCode(event.code, event.shiftKey)) || event.key;
  const single = Array.from(key).length === 1;
  // A character already says Shift (Q, ?); a letter with a modifier is a
  // capital either way, so there Shift says itself, as on other keys.
  const shift = event.shiftKey && (!single || (mods !== "" && /^[a-z]$/i.test(key))) ? "⇧" : "";
  if (single && mods) key = key.toUpperCase();
  return `${mods}${shift}${key}`;
}

export type KeyHandlers = Readonly<Record<string, (event: KeyboardEvent) => void>>;

/** A part of the window that holds the keys while it is shown ({@link KeysHeld}). */
type Hold = object;

// The shown screens' keys, newest last. Each entry is a ref, so a screen's
// handlers are always its latest without registering again on every render,
// and the hold it was added inside, if any.
const stack: { ref: { current: KeyHandlers }; hold: Hold | null }[] = [];

// The parts holding the keys, newest last: the newest alone answers them.
const holds: Hold[] = [];

const HoldContext = createContext<Hold | null>(null);

/**
 * Holds the keys while `children` are shown, as the terminal UI's changes
 * view does: a key reaches only the keys added inside it, never a list's or
 * the window's beneath it, whatever order they were added in. ⌘ shortcuts
 * still reach the window first.
 */
export function KeysHeld({ children }: { children: ReactNode }) {
  const [hold] = useState<Hold>(() => ({}));
  useEffect(() => {
    holds.push(hold);
    return () => {
      const at = holds.lastIndexOf(hold);
      if (at >= 0) holds.splice(at, 1);
    };
  }, [hold]);
  return createElement(HoldContext, { value: hold }, children);
}

/**
 * Adds `handlers` while the calling component is shown; the newest wins.
 * `beneath` puts them under every screen's, whenever either came: the
 * window's flows (src/flows) answer a key only where no screen does.
 */
export function useKeys(handlers: KeyHandlers, { beneath = false } = {}): void {
  const ref = useRef(handlers);
  ref.current = handlers;
  const hold = use(HoldContext);
  useEffect(() => {
    const entry = { ref, hold };
    if (beneath) stack.unshift(entry);
    else stack.push(entry);
    return () => {
      const at = stack.lastIndexOf(entry);
      if (at >= 0) stack.splice(at, 1);
    };
  }, [beneath, hold]);
}

/** What answers `chord` now: the newest keys added, inside the part holding the keys if one does. */
function handlerFor(chord: string): KeyHandlers[string] | undefined {
  const holding = holds[holds.length - 1] ?? null;
  for (let at = stack.length - 1; at >= 0; at -= 1) {
    const entry = stack[at];
    if (!entry || (holding && entry.hold !== holding)) continue;
    const handler = entry.ref.current[chord];
    if (handler) return handler;
  }
  return undefined;
}

function editable(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null;
  if (!element || !("tagName" in element)) return false;
  return (
    element.isContentEditable ||
    element.tagName === "INPUT" ||
    element.tagName === "TEXTAREA" ||
    element.tagName === "SELECT"
  );
}

const GLOBAL: Readonly<Record<string, (app: App) => void>> = {
  // The palette goes to or does what the daemon knows: not while it is out of reach.
  "⌘K": (app) => {
    if (app.daemon.store.getState().link.state === "up")
      app.actions.openPalette(!app.ui.getState().paletteOpen);
  },
  // The tabs in the terminal UI's order (Sessions, Needs you, Tickets, Settings).
  "⌘1": (app) => app.actions.show("all"),
  "⌘2": (app) => app.actions.show("needs-you"),
  "⌘3": (app) => app.actions.show("tickets"),
  "⌘4": (app) => app.actions.show("settings"),
  "⌘,": (app) => app.actions.show("settings"),
  "⌘\\": (app) => app.actions.foldSidebar(),
};

/**
 * Does what `chord` does on the screen shown, as if pressed: the command
 * palette runs a screen's action this way, through the screen's own keys.
 * Whether anything did.
 */
export function pressKey(chord: string): boolean {
  const handler = handlerFor(chord);
  if (!handler) return false;
  handler(new KeyboardEvent("keydown", { key: chord }));
  return true;
}

export function installKeys(app: App): () => void {
  const listener = (event: KeyboardEvent) => {
    const chord = chordOf(event);
    const ui = app.ui.getState();
    if (ui.note?.error) app.actions.clearNote();

    // The palette keeps the keys, as a dialog does its own (ui/Dialog.tsx):
    // nothing behind it moves, but ⌘K puts it away.
    if (ui.paletteOpen) {
      if (chord === "⌘K" && ui.paletteOpen) {
        event.preventDefault();
        app.actions.openPalette(false);
      }
      return;
    }
    const global = GLOBAL[chord];
    if (global) {
      event.preventDefault();
      global(app);
      return;
    }
    if (ui.keysIn === "terminal") {
      if (chord === "Q" || chord === "⇧Q") {
        event.preventDefault();
        app.actions.keysTo("list");
      }
      return;
    }
    if (ui.keysIn === "dialog" || editable(event.target)) return;

    const handler = handlerFor(chord);
    if (handler) {
      event.preventDefault();
      handler(event);
    }
  };
  window.addEventListener("keydown", listener);
  return () => window.removeEventListener("keydown", listener);
}
