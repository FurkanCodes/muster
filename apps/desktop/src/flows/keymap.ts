// The keys of the actions the flows open, as the person set them (`[keys]`
// in Muster's own settings, `muster_core::keys`): each as the window's key
// listener names it (`chordOf`) and as a key cap shows it. Muster's defaults
// where the person set none. feature/desktop-settings builds the whole
// keymap (src/keys/keymap.ts); this reads only what the flows need, the
// same way, until the two meet.

import type { Preferences } from "../ipc/generated/Preferences";

/** The actions whose keys the flows bind or show, by their names in `[keys]`, and their defaults. */
export const DEFAULT_KEYS = {
  help: ["h", "?"],
  add_project: ["a"],
  new_worktree: ["w"],
  new_session: ["n"],
  show_tickets: ["i"],
  full_screen: ["f"],
  show_changes: ["d"],
  shell: ["T"],
  stop_session: ["s"],
  restart_session: ["r"],
  close: ["x"],
  rename: ["R"],
  fold: ["e"],
  checks: ["c"],
  review: ["v"],
  tidy: ["t"],
  move_branch: ["u"],
  open_pull_request: ["o"],
  pull_request_in_browser: ["b"],
  move_up: ["k"],
  move_down: ["j"],
  last_alarm: ["l"],
  refresh_tickets: ["r"],
  another_agent: ["a"],
  ticket_in_browser: ["b"],
  close_ticket: ["x"],
  read_settings: ["r"],
  add_plugin: ["a"],
  update_plugin: ["u"],
  remove_plugin: ["x"],
  put_back: ["z"],
} as const satisfies Record<string, readonly string[]>;

export type KeyAction = keyof typeof DEFAULT_KEYS;

export interface Key {
  /** As `chordOf` names the key pressed: `R`, `⌃X`, `⇧Tab`, `Escape`. */
  chord: string;
  /** As a key cap shows it: `R`, `⌃x`, `⇧⇥`, `⎋`. */
  label: string;
}

const NAMED: Record<string, { chord: string; label: string }> = {
  tab: { chord: "Tab", label: "⇥" },
  backtab: { chord: "⇧Tab", label: "⇧⇥" },
  esc: { chord: "Escape", label: "⎋" },
  escape: { chord: "Escape", label: "⎋" },
  backspace: { chord: "Backspace", label: "⌫" },
  enter: { chord: "Enter", label: "⏎" },
  return: { chord: "Enter", label: "⏎" },
  space: { chord: " ", label: "space" },
  up: { chord: "ArrowUp", label: "↑" },
  down: { chord: "ArrowDown", label: "↓" },
  left: { chord: "ArrowLeft", label: "←" },
  right: { chord: "ArrowRight", label: "→" },
  pageup: { chord: "PageUp", label: "PgUp" },
  pagedown: { chord: "PageDown", label: "PgDn" },
  home: { chord: "Home", label: "Home" },
  end: { chord: "End", label: "End" },
};

/** A key as `[keys]` writes it (`R`, `ctrl-x`, `shift-tab`, `f5`); null where it is not one. */
export function parseKey(written: string): Key | null {
  let rest = written.trim();
  let ctrl = false;
  let alt = false;
  let shift = false;
  for (;;) {
    const lower = rest.toLowerCase();
    const prefix = ["ctrl-", "alt-", "shift-"].find(
      (mod) => lower.startsWith(mod) && lower.length > mod.length,
    );
    if (!prefix) break;
    if (prefix === "ctrl-") ctrl = true;
    else if (prefix === "alt-") alt = true;
    else shift = true;
    rest = rest.slice(prefix.length);
  }
  const mods = `${ctrl ? "⌃" : ""}${alt ? "⌥" : ""}`;
  const chars = Array.from(rest);
  if (chars.length === 1 && chars[0] !== undefined && chars[0] >= " ") {
    const typed = shift ? chars[0].toUpperCase() : chars[0];
    // The listener names a letter with a modifier by its capital, as ⌘K.
    return { chord: `${mods}${mods ? typed.toUpperCase() : typed}`, label: `${mods}${typed}` };
  }
  const lower = rest.toLowerCase();
  const named = lower === "tab" && shift ? NAMED.backtab : NAMED[lower];
  if (named) {
    const shifted = shift && lower !== "tab" ? "⇧" : "";
    return { chord: `${mods}${shifted}${named.chord}`, label: `${mods}${shifted}${named.label}` };
  }
  const f = /^f(\d{1,2})$/.exec(lower);
  if (f && Number(f[1]) >= 1 && Number(f[1]) <= 24) {
    const name = `F${f[1]}`;
    return {
      chord: `${mods}${shift ? "⇧" : ""}${name}`,
      label: `${mods}${shift ? "⇧" : ""}${name}`,
    };
  }
  return null;
}

/** The keys of `action`: the person's, or Muster's where they set none. */
export function keysOf(preferences: Preferences | null, action: KeyAction): Key[] {
  const written = preferences?.keys[action] ?? DEFAULT_KEYS[action];
  return written.map(parseKey).filter((key): key is Key => key !== null);
}

/** The keys of `action` as a hint shows them, `h ?`; `none` where the person took them away. */
export function labelOf(preferences: Preferences | null, action: KeyAction): string {
  const keys = keysOf(preferences, action);
  return keys.length > 0 ? keys.map((key) => key.label).join(" ") : "none";
}
