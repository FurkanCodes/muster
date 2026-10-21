# KeyRow

One action and its keys on Settings › Keys. Muster is driven from the keyboard, and every key is the person's to change.

**Anatomy.** `mu-keyrow`: `size-row` (32px) at least, `space-4` by `space-12` padding, `radius-md`, hover `surface-raised`. On the left is what the action does in `body`. A key the person set has a **Label** "yours". On the right are its keys as **Kbd**s, `space-4` apart. A plugin's panel is listed under its plugin's name.

**Changing a key.** ⏎ or a click puts the row in waiting (`is-waiting`): `surface-lit` with a 1px `accent` frame inside, and "Press a key… ⎋ cancels" in `accent` where the keys were. The next key pressed is set, unless it is refused. A refused key shows under the row (`mu-keyrow-error`, `waiting`), naming both actions: "d already shows Changes on this screen." ⌫ puts an action's keys back as they were; ⇧R puts every key back.

**Refused:** a key another action has on a screen they share; one every screen keeps (⌘K, ⇧Q); and, for an action inside a terminal, a key the program could take as typing (neither ⌘, ⌃ nor ⌥).

**Groups.** Rows are grouped by the screen the action works on (*Everywhere*, *Sessions and Needs you*, *Inside a terminal*, *Tickets*, *Settings*, then *Plugins' panels*), each under a `meta-strong` title. A field at the top narrows the rows by what they do or by their key.
