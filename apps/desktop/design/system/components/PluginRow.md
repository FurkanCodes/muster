# PluginRow

One plugin in Settings › Plugins: what it is, where it is from, whether it runs, and what it brings.

**Anatomy.** `mu-plugin`: at least `size-row-two` (44px) high, with `space-8` by `space-12` padding, `radius-md`, and a hover fill. On the left is a button (`mu-plugin-open`) that shows the plugin's details. It holds:
- the name in `body`, then its version in `code` and `text-quiet`, then a **Label** saying where it is from (built in, git, folder);
- under the name, in `meta`, what it brings ("panel: graph (G)", "tickets: YouTrack", "hooks: needs you, finished, failed"). If it cannot run, this line says why instead, in `waiting` (`is-off`): "Off: changed since you approved it". If someone changed it from the command line, the line says that and when.

On the right is its **Switch**. Turning on a plugin that cannot run is refused, and the row says why. The row whose details are shown is `is-selected` (`surface-lit`).

**Order.** Muster's own plugins first, then the person's, by name; then a row to add one.

**The consumer provides** the plugin as the daemon lists it. Muster's own plugins can be turned off but not removed.
