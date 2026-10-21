// Keys: Muster is driven from the keyboard first (README: Keys). One
// listener on the window for the whole app; screens add their keys while
// they are shown. While a terminal has the keys, every key goes to its
// program but the one that leaves it (⇧Q) and ⌘ shortcuts.

import { useEffect, useRef } from "react";
import type { App } from "./context";

/** A key as Muster names it: `d`, `T`, `Enter`, `⌘K`, `⇧Tab`. */
export function chordOf(
  event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">,
): string {
  const mods = `${event.metaKey ? "⌘" : ""}${event.ctrlKey ? "⌃" : ""}${event.altKey ? "⌥" : ""}`;
  const single = event.key.length === 1;
  // A capital letter already says Shift; other keys say it with ⇧.
  const shift = event.shiftKey && !(single && event.key !== event.key.toLowerCase()) ? "⇧" : "";
  const key = single && mods ? event.key.toUpperCase() : event.key;
  return `${mods}${shift}${key}`;
}

export type KeyHandlers = Readonly<Record<string, (event: KeyboardEvent) => void>>;

// The shown screens' keys, newest last. Each entry is a ref, so a screen's
// handlers are always its latest without registering again on every render.
const stack: { current: KeyHandlers }[] = [];

/**
 * Adds `handlers` while the calling component is shown; the newest wins.
 * `beneath` puts them under every screen's, whenever either came: the
 * window's flows (src/flows) answer a key only where no screen does.
 */
export function useKeys(handlers: KeyHandlers, { beneath = false } = {}): void {
  const ref = useRef(handlers);
  ref.current = handlers;
  useEffect(() => {
    if (beneath) stack.unshift(ref);
    else stack.push(ref);
    return () => {
      const at = stack.lastIndexOf(ref);
      if (at >= 0) stack.splice(at, 1);
    };
  }, [beneath]);
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
  "⌘1": (app) => app.actions.show("needs-you"),
  "⌘2": (app) => app.actions.show("all"),
  "⌘3": (app) => app.actions.show("tickets"),
  "⌘,": (app) => app.actions.show("settings"),
  "⌘\\": (app) => app.actions.foldSidebar(),
};

/**
 * Does what `chord` does on the screen shown, as if pressed: the command
 * palette runs a screen's action this way, through the screen's own keys.
 * Whether anything did.
 */
export function pressKey(chord: string): boolean {
  for (let at = stack.length - 1; at >= 0; at -= 1) {
    const handler = stack[at]?.current[chord];
    if (handler) {
      handler(new KeyboardEvent("keydown", { key: chord }));
      return true;
    }
  }
  return false;
}

export function installKeys(app: App): () => void {
  const listener = (event: KeyboardEvent) => {
    const chord = chordOf(event);
    const ui = app.ui.getState();
    if (ui.note?.error) app.actions.clearNote();

    // A dialog or the palette keeps the keys (design system: Dialog):
    // nothing behind it moves, but ⌘K puts the palette away.
    if (ui.keysIn === "dialog" || ui.paletteOpen) {
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
    if (editable(event.target)) return;

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
