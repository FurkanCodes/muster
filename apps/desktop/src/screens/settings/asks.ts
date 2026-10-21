// What Settings asks of `muster` and of the Mac, each on the person's word,
// done by the app's Rust side (`src-tauri/src/settings_actions.rs`):
// restarting the daemon in place, as `muster reload` does; the Mac's
// notification settings for Muster; and Muster's own settings.toml in the
// person's editor. A bridge that cannot does none of them, and says so;
// the mock records what it was asked.

import type { Bridge } from "../../ipc/bridge";

/** A bridge that can do Settings' asks. */
export interface SettingsAsks {
  /** Runs `muster reload`; what it said. */
  restartInPlace(): Promise<string>;
  openNotificationSettings(): Promise<void>;
  editSettingsFile(): Promise<void>;
}

export function canAsk(bridge: Bridge): bridge is Bridge & SettingsAsks {
  return typeof (bridge as Partial<SettingsAsks>).restartInPlace === "function";
}
