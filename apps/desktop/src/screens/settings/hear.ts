// Hearing a sound as it is chosen (PRODUCT.md §6: "heard as they choose
// it"): one of the Mac's own or the person's, by name, played by the
// app's Rust side (`src-tauri/src/sounds.rs`) with `afplay`, as the terminal
// UI plays it. A bridge that cannot play one plays nothing; the mock
// records what it was asked.

import { useActions, useBridge } from "../../app/context";
import type { Bridge } from "../../ipc/bridge";
import type { Sound } from "../../ipc/generated/Sound";
import { heard } from "./words";

/** A bridge that can play a sound by its name. */
export interface Hearing {
  hear(sound: string): Promise<void>;
}

function canHear(bridge: Bridge): bridge is Bridge & Hearing {
  return typeof (bridge as Partial<Hearing>).hear === "function";
}

/** Plays `sound` where it is a sound of its own, saying so; the Mac's alert and none are not. */
export function useHear(): (sound: Sound) => void {
  const bridge = useBridge();
  const ui = useActions();
  return (sound) => {
    if (!heard(sound) || !canHear(bridge)) return;
    ui.tell(`Playing ${sound}`);
    void bridge.hear(sound).catch(() => {});
  };
}
