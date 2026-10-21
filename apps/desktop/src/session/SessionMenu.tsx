// The pane header's more menu (design system: Menu): what can be done with
// the session, each with its key. ↑↓ move, ⏎ does it, ⎋ closes, and an
// item's own key does it at once. While it is open its keys are its own.

import { type KeyboardEvent, useEffect, useRef } from "react";
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

export function SessionMenu({
  label,
  items,
  onClose,
}: {
  label: string;
  items: readonly MenuItem[];
  onClose: () => void;
}) {
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const opener = document.activeElement;
    menu.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    return () => {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, []);
  const choose = (item: MenuItem) => {
    onClose();
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
      onClose();
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
      className="mu-menu session-menu"
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
