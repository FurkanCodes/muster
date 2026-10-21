# TitleBar

The window's top edge: how the flock stands, and the way to anything by typing.

**Anatomy.** `layout-titlebar` (44px) high on `surface-rail`, with a `line` hairline below. The left `layout-traffic-lights` (78px) stays free for the Mac's window buttons, which the app draws natively. Then, `space-12` apart: **Kep**, the command field (`mu-command`, `size-control-lg` high, growing to `size-command`, with ⌘K at its end), a spacer, the flock count, and the memory meter.

**Flock count** (`mu-flock`). Working, waiting and done, each as its status mark and a number in its status colour, `space-12` apart. Each has an `aria-label` in words ("3 working, 2 waiting on you, 4 done").

**Memory meter** (`mu-meter`). Eight cells, each `size-meter-cell-width` by `size-meter-cell-height`, `space-2` apart. Lit cells are `done` under 70% of the machine's memory, `working` past 70% (`mu-meter--busy`) and `waiting` past 90% (`mu-meter--full`); unlit cells are `line-strong`. Its tooltip gives the percentage.

**As it narrows,** the command field shrinks first, then Kep goes, then the meter. The flock count always stays.

- Do: drag the window by any empty part of the bar.
- Don't: put screen tabs here; screens live in the sidebar.
