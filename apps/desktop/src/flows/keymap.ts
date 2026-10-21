// The keys of the actions the flows bind and show, as the person set them:
// the window's keymap (src/keys), read here as the key listener names a
// press (`chord`) and as a key cap shows it (`label`).

import type { PluginInfo } from "../ipc/generated/PluginInfo";
import type { Preferences } from "../ipc/generated/Preferences";
import { chordLabel, domChord } from "../keys/chord";
import { chordsOf } from "../keys/keymap";
import { keymapFor } from "../keys/useKeymap";

/** The actions, by their names in `[keys]`, whose keys the flows bind or show. */
export type KeyAction =
  | "help"
  | "add_project"
  | "new_worktree"
  | "new_session"
  | "show_tickets"
  | "full_screen"
  | "show_changes"
  | "shell"
  | "stop_session"
  | "restart_session"
  | "close"
  | "rename"
  | "fold"
  | "checks"
  | "review"
  | "tidy"
  | "move_branch"
  | "open_pull_request"
  | "pull_request_in_browser"
  | "move_up"
  | "move_down"
  | "last_alarm"
  | "fold_list"
  | "next_tab"
  | "previous_tab"
  | "refresh_tickets"
  | "another_agent"
  | "ticket_in_browser"
  | "close_ticket"
  | "read_settings"
  | "add_plugin"
  | "update_plugin"
  | "remove_plugin"
  | "put_back"
  | "changes_again"
  | "close_changes"
  | "leave_session"
  | "changes_help";

export interface Key {
  /** As `chordOf` names the key pressed: `R`, `⌃E`, `⇧Tab`. */
  chord: string;
  /** As a key cap shows it. */
  label: string;
}

const NO_KEYS: Readonly<Record<string, readonly string[]>> = {};
// The flows' actions are Muster's own: plugins' panels never change their keys.
const NO_PLUGINS: readonly PluginInfo[] = [];

/** The keys of `action`: the person's, or Muster's where they set none. */
export function keysOf(preferences: Preferences | null, action: KeyAction): Key[] {
  const map = keymapFor(preferences?.keys ?? NO_KEYS, NO_PLUGINS);
  return chordsOf(map, action).map((chord) => ({
    chord: domChord(chord),
    label: chordLabel(chord),
  }));
}

/** The keys of `action` as a hint shows them, `h ?`; `none` where the person took them away. */
export function labelOf(preferences: Preferences | null, action: KeyAction): string {
  const keys = keysOf(preferences, action);
  return keys.length > 0 ? keys.map((key) => key.label).join(" ") : "none";
}
