# Button

A labelled action: one word or a short phrase that says exactly what happens ("Put back", "Close ticket", "Start session").

**Variants.** `mu-btn--primary` (accent fill) for the one action a screen or dialog exists for, at most one in view. `mu-btn--secondary` (raised fill) for the other answer in a dialog. `mu-btn--quiet` (outline, quiet text) for actions in a pane header or a strip. `mu-btn--danger` (waiting-soft fill, waiting text) for what tells other people or cannot be undone, such as closing a ticket or removing a plugin.

**Sizes.** `mu-btn--sm` is `size-control-sm` (24px), with `space-8` sides, `radius-sm` and 12px text, for strips and rows. The default is `size-control-md` (28px), with `space-10` sides, `radius-md` and `label` text. `mu-btn--lg` is `size-control-lg` (32px), with `space-12` sides, for dialogs.

**Anatomy.** An optional icon at `size-icon` comes first, then `space-8`, then the label. A key hint (**Kbd**) goes after the label only where the button lives in a strip of keyboard actions.

**States.** Hover `accent-hover` or `surface-raised`, pressed `accent-pressed` or `surface-tint`, focus ring, disabled (`opacity-disabled` plus a tooltip saying why), busy (`aria-busy="true"`: the spinner replaces the icon, the label says what is happening, a second press does nothing).

- Do: name the result ("Put back 3 files"), and repeat that word in the note that follows ("Put back").
- Don't: write "OK", "Submit" or "Yes"; put two primary buttons side by side; use danger for what can be undone.
