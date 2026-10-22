// The pointer in a list whose keys the list itself holds (a listbox or tree
// naming its selected line by `aria-activedescendant`): a press on a line
// gives the keys to the list, never to the line, so the browser's focus and
// the list's selection never part, and no ring is left on a line the
// arrows have since moved from.

import type { MouseEvent } from "react";

/**
 * On a line's mousedown: the line never takes the focus. A click's makes
 * the list it is in take it, and the click then selects as ever; a
 * right-click's (or ⌃-click's) leaves it to the menu the line opens, which
 * may open before this press is heard.
 */
export function focusListOf(event: MouseEvent<HTMLElement>): void {
  event.preventDefault();
  if (event.button !== 0 || event.ctrlKey) return;
  event.currentTarget
    .closest<HTMLElement>('[role="listbox"], [role="tree"]')
    ?.focus({ preventScroll: true });
}
