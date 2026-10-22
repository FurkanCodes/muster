// The pane header's more menu (design system: Menu): what can be done with
// the session, each with its key. ↑↓ move, ⏎ does it, ⎋ closes, and an
// item's own key does it at once. While it is open its keys are its own.
// Asked for by a right-click on the session's line, it opens where the
// pointer is, kept inside the window.

import { type KeyboardEvent, useEffect, useLayoutEffect, useRef } from "react";
import { chordOf } from "../app/keys";
import { domChord } from "../keys/chord";
import { chordsOf, type Keymap, labelOf } from "../keys/keymap";
import { Kbd } from "../ui/controls";

export interface MenuItem {
  /** Its keys, as a key cap shows them. */
  key: string;
  /** Its keys as the key listener names a press (`chordOf`); `key` where there are none. */
  chords?: readonly string[];
  label: string;
  does: () => void;
  /** Destructive: last, after a separator, in the waiting colour. */
  danger?: boolean;
}

/** `action`'s keys as the person set them, for a menu item. */
export function keysOf(map: Keymap, action: string): Pick<MenuItem, "key" | "chords"> {
  return { key: labelOf(map, action), chords: chordsOf(map, action).map(domChord) };
}

/** Room kept between a menu opened at the pointer and the window's edges. */
const EDGE = 8;

export function SessionMenu({
  label,
  items,
  onClose,
  at,
}: {
  label: string;
  items: readonly MenuItem[];
  onClose: () => void;
  /** Where in the window to open it (a right-click); under its button where not given. */
  at?: { x: number; y: number };
}) {
  const menu = useRef<HTMLDivElement>(null);
  // At the pointer, moved back inside the window where it would run past an edge.
  useLayoutEffect(() => {
    const element = menu.current;
    if (!at || !element) return;
    const { width, height } = element.getBoundingClientRect();
    const left = Math.max(EDGE, Math.min(at.x, window.innerWidth - width - EDGE));
    const top = Math.max(EDGE, Math.min(at.y, window.innerHeight - height - EDGE));
    element.style.left = `${left}px`;
    element.style.top = `${top}px`;
  }, [at]);
  // What had the focus before it, given it back as the menu is done with
  // (⎋, or an item chosen), not merely as it goes: one closed by a click
  // elsewhere leaves the focus where the click put it, and a mount React
  // runs twice in development never pulls the focus out of the menu.
  const opener = useRef<Element | null>(null);
  useEffect(() => {
    opener.current ??= document.activeElement;
    menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, []);
  const done = () => {
    onClose();
    const back = opener.current;
    if (back instanceof HTMLElement && back.isConnected) back.focus();
  };
  const choose = (item: MenuItem) => {
    done();
    item.does();
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey) return;
    // The queue behind it hears none of the menu's keys.
    event.stopPropagation();
    const entries = [...(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const at = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.preventDefault();
      done();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const by = event.key === "ArrowDown" ? 1 : entries.length - 1;
      entries[(at + by) % entries.length]?.focus();
    } else {
      const pressed = chordOf(event);
      const item = items.find((candidate) =>
        (candidate.chords ?? [candidate.key]).includes(pressed),
      );
      if (item) {
        event.preventDefault();
        choose(item);
      }
    }
  };
  const plain = items.filter((item) => !item.danger);
  const danger = items.filter((item) => item.danger);
  const entry = (item: MenuItem) => (
    <button
      key={item.key}
      type="button"
      role="menuitem"
      className={`mu-menu-item ${item.danger ? "mu-menu-item--danger" : ""}`}
      onClick={() => choose(item)}
    >
      {item.label}
      <Kbd>{item.key}</Kbd>
    </button>
  );
  return (
    <div
      ref={menu}
      role="menu"
      aria-label={label}
      className={`mu-menu session-menu ${at ? "session-menu--at" : ""}`}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) onClose();
      }}
    >
      {plain.map(entry)}
      {danger.length && plain.length ? <hr className="mu-menu-sep" /> : null}
      {danger.map(entry)}
    </div>
  );
}
