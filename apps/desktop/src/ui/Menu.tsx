// A menu of actions (design system: Menu), on Base UI's menu: it
// keeps the focus while open, moves by ↑↓ (and round), finds an item by
// its first letters, closes on ⎋ or a click outside and gives the focus
// back to what opened it. Each item also does itself on its own key, as
// the keymap names it, and while the menu is open none of its keys reach
// the screen behind it (⌘ shortcuts still do).
//
//   <ActionMenu label={title} items={items} trigger={<IconButton icon="more" label="More" />} />
//   <ActionMenu label={title} items={items} at={{ x, y }} onClose={…} />   (a right-click's)

import { Menu } from "@base-ui/react/menu";
import { type KeyboardEvent, type ReactElement, useRef, useState } from "react";
import { chordOf } from "../app/keys";
import { domChord } from "../keys/chord";
import { chordsOf, type Keymap, labelOf } from "../keys/keymap";
import { cn } from "../lib/cn";
import { Kbd } from "./controls";
import { portalRoot } from "./portal";

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

export function ActionMenu({
  label,
  items,
  trigger,
  at,
  open,
  onOpenChange,
  onClose,
}: {
  /** Its name, for a screen reader: what it acts on. */
  label: string;
  items: readonly MenuItem[];
  /** The button that opens it, under which it opens. */
  trigger?: ReactElement;
  /** Instead: where in the window it opens (a right-click), open while it is drawn. */
  at?: { x: number; y: number };
  /** Opened and closed by its owner, where its trigger's own press is not the only way. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** It closed: an item chosen, ⎋, or a click outside. */
  onClose?: () => void;
}) {
  // What had the focus as it opened: given back as it closes where it opened at a point.
  const opener = useRef<Element | null>(at ? document.activeElement : null);
  // Its own open state where its owner keeps none: an item's own key closes it as a click does.
  const [own, setOwn] = useState(false);
  const shown = at ? true : (open ?? own);
  // Where it is drawn, found as it opens, as Select's list is: a menu in the
  // window's first render (the projects' +) is drawn before the window's
  // root is there, and a value worked out while drawing would be kept.
  const [container, setContainer] = useState(portalRoot);
  // Opened by its owner (a key), not by its trigger: found then.
  if (shown && !container) {
    const root = portalRoot();
    if (root) setContainer(root);
  }
  const change = (next: boolean) => {
    if (next) setContainer(portalRoot());
    if (open === undefined) setOwn(next);
    onOpenChange?.(next);
    if (!next) onClose?.();
  };
  const plain = items.filter((item) => !item.danger);
  const danger = items.filter((item) => item.danger);
  const choose = (item: MenuItem) => {
    change(false);
    item.does();
  };
  // An item's own key does it before the menu's finding by first letters
  // takes the letter.
  const onKeyDownCapture = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey) return;
    const pressed = chordOf(event);
    const item = items.find((candidate) => (candidate.chords ?? [candidate.key]).includes(pressed));
    if (item) {
      event.preventDefault();
      event.stopPropagation();
      choose(item);
    }
  };
  // The screen behind it hears none of the menu's keys.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!event.metaKey) event.stopPropagation();
  };
  const entry = (item: MenuItem) => (
    <Menu.Item
      key={item.key + item.label}
      className={cn("mu-menu-item", item.danger && "mu-menu-item--danger")}
      // Found by its words as typed, not its key cap's.
      label={item.label}
      onClick={() => item.does()}
    >
      {item.label}
      {item.key ? <Kbd>{item.key}</Kbd> : null}
    </Menu.Item>
  );
  // Where a right-click opened it: a point the menu hangs from.
  const anchor = useRef<HTMLSpanElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  return (
    <Menu.Root open={shown} onOpenChange={(next) => change(next)} modal={false}>
      {at ? (
        <span
          ref={anchor}
          className="ui-menu-anchor"
          style={{ left: at.x, top: at.y }}
          aria-hidden="true"
        />
      ) : (
        <Menu.Trigger render={trigger} />
      )}
      <Menu.Portal container={container}>
        <Menu.Positioner
          className="ui-menu"
          side="bottom"
          align={at ? "start" : "end"}
          sideOffset={at ? 0 : 4}
          collisionPadding={8}
          anchor={at ? anchor : undefined}
        >
          <Menu.Popup
            className="mu-menu"
            // Named for what it acts on, not by its button.
            aria-label={label}
            aria-labelledby={undefined}
            onKeyDownCapture={onKeyDownCapture}
            onKeyDown={onKeyDown}
            ref={popup}
            finalFocus={() => {
              // An item that took the focus on (a name to type) keeps it there.
              const now = document.activeElement;
              if (now && now !== document.body && !popup.current?.contains(now)) return false;
              const back = opener.current;
              return at && back instanceof HTMLElement && back.isConnected ? back : true;
            }}
          >
            {plain.map(entry)}
            {danger.length && plain.length ? <Menu.Separator className="mu-menu-sep" /> : null}
            {danger.map(entry)}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}
