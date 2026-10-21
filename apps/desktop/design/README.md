# The design, for reference

What the app should look like, copied from the design canvas and the Muster design system so every branch can read it without either. Nothing here ships.

- `screens/`: each board of the canvas as HTML, its markup using the design system's own `mu-*` classes (`src/styles/components.css`). Build each screen to match its board: layout, classes, copy, states. `Triage.html` is the chosen main window; `Spec.html` measures it. `Main.html` (Cockpit) and `Flock.html` are directions not taken.
- `system/README.md`: the design system's rules (voice, colour, type, space, states, motion, icons, keys); `system/layout.md`, the window measured; `system/components/`, each component's guidelines.

The boards load the design system's files from `ds/muster/` and logos from the canvas, so opened as files they show unstyled; read them as markup.
