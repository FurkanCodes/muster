# Menu

What can be done with the thing under the pointer, each with its key: a right-click on a queue item, a row or a file; the more button in the pane header.

**Anatomy.** `mu-menu`: at least `size-menu-min` (220px) wide, `space-4` inset, `radius-xl`, `surface-panel`, `shadow-popover`, on `z-menu`. Items (`mu-menu-item`) are `size-control-md` (28px) high, `space-8` sides, `radius-md` (the menu's radius less its inset), `body` text, with a `size-logo-sm` logo or `size-icon` icon first where it helps and the item's **Kbd** right-aligned. An optional label (`mu-menu-label`, `meta`) names what the menu is about. Separators (`mu-menu-sep`) are `line` hairlines with `space-4` above and below.

**States.** Hover and the keyboard's active item (`is-active`) fill `surface-lit`. A destructive item (`mu-menu-item--danger`) is `waiting` and comes last, after a separator.

**Keys.** ↑↓ move, ⏎ does it, ⎋ closes, and an item's own key does it at once. The menu opens just below the pointer and never off the window's edge, sliding in over `timing-slow`.
