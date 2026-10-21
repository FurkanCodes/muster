import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { sandbox } from "./harness";

// What the app did that would reach outside it in a real build. Built
// with the `e2e` feature and given MUSTER_E2E_RECORD, it writes each such
// request here instead (src-tauri/src/e2e.rs): a web address it was asked
// to open, a sound it was asked to play, a file it was asked to show. The
// stand-in `muster` (support/stand-ins.ts) writes each start of the daemon.

function lines(name: string): string[] {
  const file = join(sandbox().record, name);
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean) : [];
}

/** The web addresses the app was asked to open in the browser, oldest first. */
export function opened(): string[] {
  return lines("opened");
}

/** The sounds the app was asked to play, by the files it would play. */
export function heard(): string[] {
  return lines("heard");
}

/** The files the app was asked to show (the daemon's log). */
export function revealed(): string[] {
  return lines("revealed");
}

/** Each time the app started the daemon (`muster daemon --detach`). */
export function daemonStarts(): string[] {
  return lines("daemon-starts");
}

/**
 * Holds the daemon out of reach (`true`): the app's `muster` refuses to
 * start it, as a broken install would; `false` lets it start again.
 */
export function holdDaemon(held: boolean): void {
  const flag = join(sandbox().record, "daemon-refused");
  if (held) writeFileSync(flag, "");
  else rmSync(flag, { force: true });
}
