# Kep

Muster's sheepdog, in the title bar, saying in one face how the flock stands. Muster gathers sessions as a sheepdog musters a flock.

**Faces,** set in `font-mono` at 13px, in order of what wins:

| Face | When | Colour |
|---|---|---|
| `▲•ᴥ•▲!` barking | a session asks or wants a look; the `!` pulses | `waiting` |
| `▼^ᴥ^▼` glad | for three seconds after a session finishes | `done` |
| `▼•ᴥ•▼` watching | something works; blinks for two steps in forty | `text` |
| `▼-ᴥ-▼ᶻ` asleep | nothing works or waits | `text-quiet` |
| `▼;ᴥ;▼` away | the daemon is out of reach | `text-quiet` |

Its tooltip says the same in words ("Something asks for you").

- Do: let Kep go first when the title bar is short of room.
- Don't: draw Kep anywhere else in the interface, or as a picture.
