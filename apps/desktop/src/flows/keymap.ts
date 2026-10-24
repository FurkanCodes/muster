// The keys of the actions the flows bind and show, as the person set them:
// the window's keymap (src/keys), read here as the key listener names a
// press (`chord`) and as a key cap shows it (`label`).

import type { ActionName } from "../ipc/generated/ActionName";
import { chordLabel, domChord } from "../keys/chord";
import { chordsOf, type Keymap } from "../keys/keymap";
import { keymapFor } from "../keys/useKeymap";
import type { DaemonState } from "../state/daemon";

/** The actions, by their names in `[keys]`, whose keys the flows bind or show. */
export type KeyAction = ActionName;

export interface Key {
  /** As `chordOf` names the key pressed: `R`, `⌃E`, `⇧Tab`. */
  chord: string;
  /** As a key cap shows it. */
  label: string;
}

const NO_KEYS: Readonly<Record<string, readonly string[]>> = {};

/** What a keymap is read from: the daemon's state, the person's keys and the plugins' panels. */
export type Keyed = Pick<DaemonState, "preferences" | "plugins">;

/** The window's keymap, as `useKeymap` gives it, from `state`. */
function mapOf(state: Keyed): Keymap {
  return keymapFor(state.preferences?.keys ?? NO_KEYS, state.plugins);
}

/** The keys of `action` in `map`. */
export function keysIn(map: Keymap, action: KeyAction): Key[] {
  return chordsOf(map, action).map((chord) => ({
    chord: domChord(chord),
    label: chordLabel(chord),
  }));
}

/** The keys of `action`: the person's, or Muster's where they set none. */
export function keysOf(state: Keyed, action: KeyAction): Key[] {
  return keysIn(mapOf(state), action);
}

/** The keys of `action` as a hint shows them, `h ?`; `none` where the person took them away. */
export function labelOf(state: Keyed, action: KeyAction): string {
  const keys = keysOf(state, action);
  return keys.length > 0 ? keys.map((key) => key.label).join(" ") : "none";
}
