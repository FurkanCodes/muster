// Keys: Muster is driven from the keyboard first (README: Keys). One
// listener on the window for the whole app; screens add their keys while
// they are shown. While a terminal has the keys, every key goes to its
// program but the one that leaves it (⇧Q) and ⌘ shortcuts.

import { useEffect, useRef } from "react";
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

// The shown screens' keys, newest last. Each entry is a ref, so a screen's
// handlers are always its latest without registering again on every render.
const stack: { current: KeyHandlers }[] = [];

/** Adds `handlers` while the calling component is shown; the newest wins. */
export function useKeys(handlers: KeyHandlers): void {
  const ref = useRef(handlers);
  ref.current = handlers;
  useEffect(() => {
    stack.push(ref);
    return () => {
      const at = stack.lastIndexOf(ref);
      if (at >= 0) stack.splice(at, 1);
    };
  }, []);
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
  "⌘K": (app) => app.actions.openPalette(!app.ui.getState().paletteOpen),
  "⌘1": (app) => app.actions.show("needs-you"),
  "⌘2": (app) => app.actions.show("all"),
  "⌘3": (app) => app.actions.show("tickets"),
  "⌘,": (app) => app.actions.show("settings"),
  "⌘\\": (app) => app.actions.foldSidebar(),
};

export function installKeys(app: App): () => void {
  const listener = (event: KeyboardEvent) => {
    const chord = chordOf(event);
    const ui = app.ui.getState();
    if (ui.note?.error) app.actions.clearNote();

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
    if (ui.keysIn === "dialog" || ui.paletteOpen || editable(event.target)) return;

    for (let at = stack.length - 1; at >= 0; at -= 1) {
      const handler = stack[at]?.current[chord];
      if (handler) {
        event.preventDefault();
        handler(event);
        return;
      }
    }
  };
  window.addEventListener("keydown", listener);
  return () => window.removeEventListener("keydown", listener);
}
