# Switch

A setting that is on or off and takes effect at once, on its row in Settings.

**Anatomy.** The track is `size-switch-width` by `size-switch-height` (28 by 16), `radius-full`, `line-control` when off and `accent` when on. The knob is 12px, 2px in, `text` when off and `on-accent` when on, sliding over `timing-base`. It is a `button` with `role="switch"` and `aria-checked`.

**Settings row** (`mu-setting`). The whole row is the target: `size-row-two` (44px) at least, `space-8` by `space-12` padding, `radius-md`, hover `surface-raised`. The setting's name in `body` is over a line of `meta` saying what it does or what it costs ("Asks your agent with your own sign-in; sends only the first prompt"). The switch is at the right.

- Do: apply the change at once and say so in the status bar ("Notifications on").
- Don't: use a switch for something that needs a Save, or for a choice of more than two.
