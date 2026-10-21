# IconButton

An action shown by its icon alone, for well-known actions in tight places: full screen, more, close, new session.

**Sizes.** The default is `size-control-md` (28px) with a `size-icon-lg` (16px) icon, for pane headers and the sidebar. `mu-icon-btn--sm` is `size-control-sm` (24px) with a `size-icon` (14px) icon, inside rows. Corners are `radius-sm`. The resting colour is `text-quiet`; hover fills `surface-raised` and lifts the icon to `text`.

**The consumer provides** an `aria-label` that names the action and a tooltip with its key ("Full screen (f)"). A toggle sets `aria-pressed`; one that opens a menu sets `aria-expanded`, and both keep the hover fill while on.

- Do: use it only where the icon is unmistakable. If an icon needs explaining, use a **Button** with a label.
- Don't: leave out the label for screen readers, or colour the icon with a status colour.
