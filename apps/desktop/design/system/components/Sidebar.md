# Sidebar

The screens, and the projects to narrow them by. It is the only way between screens; there are no tabs.

**Anatomy.** `layout-sidebar` (220px) wide on `surface-rail`, `space-12` top and bottom, `space-8` sides, a `line` hairline at its right. From the top:
1. The queues, `space-2` apart: *Needs you* (▲, its count in `waiting`), *Not seen* (the `news` dot), *Working* (the spinner), *All sessions*.
2. A *Projects* label (`mu-nav-label`, `meta-strong`, `space-16` above it), then each project with its swatch (`size-swatch`, its `project-*` colour), its name in `text`, and its count of sessions. Choosing a project narrows the queue to it; choosing it again shows all.
3. At the foot: *Tickets* and *Settings*.

**Nav item** (`mu-nav-item`). `size-control-lg` (32px) high, `space-10` sides and gap, `radius-sm`, `label` text. Its icon or mark sits in a `size-icon-lg` column so the names line up. Resting `text-quiet`, hover `surface-raised`, current (`aria-current="page"`) `surface-lit` with `text`. The count is right-aligned in `meta-strong`, `waiting` for *Needs you*.

**Folded** (`mu-nav--folded`, `layout-sidebar-folded`, 52px): marks and counts only, each name in a tooltip. It folds on ⌘\ or as the window narrows (see **Window layout**).
