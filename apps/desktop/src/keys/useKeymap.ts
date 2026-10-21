// The keymap as the screens read it: the person's keys as the daemon keeps
// them and the panels of the plugins that run, made once for each change
// to either and shared by every component that asks.

import { useDaemon } from "../app/context";
import { type KeyHandlers, useKeys } from "../app/keys";
import type { PluginInfo } from "../ipc/generated/PluginInfo";
import type { ScreenId } from "../state/ui";
import { type Keymap, type KeyScreen, keymapOf, labelOf, panelsOf, screenKeys } from "./keymap";

const NONE: Readonly<Record<string, string[]>> = {};

let last: {
  keys: Readonly<Record<string, readonly string[]>>;
  plugins: readonly PluginInfo[];
  map: Keymap;
} | null = null;

/** The keymap for `keys` and `plugins`; the same object while both are (js-cache-function-results). */
export function keymapFor(
  keys: Readonly<Record<string, readonly string[]>>,
  plugins: readonly PluginInfo[],
): Keymap {
  if (last?.keys !== keys || last.plugins !== plugins) {
    last = { keys, plugins, map: keymapOf(keys, panelsOf(plugins)) };
  }
  return last.map;
}

export function useKeymap(): Keymap {
  const keys = useDaemon((state) => state.preferences?.keys ?? NONE);
  const plugins = useDaemon((state) => state.plugins);
  return keymapFor(keys, plugins);
}

/** `action`'s first key, as hints and tooltips show it. */
export function useKeyLabel(action: string): string {
  return labelOf(useKeymap(), action);
}

/** The table's screen for each of the window's lists: the queues are its *Needs you*. */
export const KEY_SCREENS: Readonly<Record<ScreenId, KeyScreen>> = {
  "needs-you": "needs_you",
  "not-seen": "needs_you",
  working: "needs_you",
  all: "sessions",
  tickets: "tickets",
  settings: "settings",
};

/**
 * Answers the keys `screen` has for the actions in `does`, as the person
 * set them, while the calling component is shown. An action with no
 * handler here is left to the window.
 */
export function useActionKeys(
  screen: KeyScreen,
  does: Readonly<Partial<Record<string, (event: KeyboardEvent) => void>>>,
): void {
  const map = useKeymap();
  const handlers: Record<string, (event: KeyboardEvent) => void> = {};
  for (const [dom, action] of screenKeys(map, screen)) {
    const handler = does[action];
    if (handler) handlers[dom] = handler;
  }
  useKeys(handlers as KeyHandlers);
}
