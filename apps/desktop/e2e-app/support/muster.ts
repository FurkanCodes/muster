import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { browser } from "@wdio/globals";
import { sandbox } from "./harness";
import { MUSTER, sandboxEnv } from "./sandbox";

// The daemon as the person's terminal sees it: `muster` with the
// sandbox's variables, and `muster status --json` read for what the tests
// assert about the daemon's state.

/** Runs `muster` in the sandbox, as the person's terminal would; its output. Throws on failure, with what it said. */
export function muster(...args: string[]): string {
  try {
    return execFileSync(MUSTER, args, {
      env: sandboxEnv(sandbox()),
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
  } catch (error) {
    const failed = error as { stderr?: string; stdout?: string };
    throw new Error(
      `muster ${args.join(" ")} failed: ${failed.stderr ?? ""}${failed.stdout ?? ""}`,
    );
  }
}

export interface StatusSession {
  id: string;
  project: string;
  workspace: string;
  branch: string | null;
  harness: string;
  title: string;
  status: string;
  needs: string | null;
  unseen: boolean;
}

export interface StatusWorkspace {
  id: string;
  kind: string;
  path: string;
  branch: string | null;
  stacked_on: string | null;
  sessions: StatusSession[];
}

export interface StatusProject {
  id: string;
  name: string;
  root: string;
  workspaces: StatusWorkspace[];
}

export interface Status {
  running: boolean;
  daemon?: { version: string; pid: number; protocol: number };
  health?: { tasks: number; work: number; clients: number; terminals: number };
  projects?: StatusProject[];
}

/** `muster status --json`; not running where it cannot be read. */
export function status(): Status {
  try {
    return JSON.parse(muster("status", "--json")) as Status;
  } catch {
    return { running: false };
  }
}

/** Every session the daemon holds. */
export function sessions(state: Status = status()): StatusSession[] {
  return (state.projects ?? []).flatMap((project) =>
    project.workspaces.flatMap((workspace) => workspace.sessions),
  );
}

/** The session titled `title`, if the daemon has one. */
export function session(title: string, state: Status = status()): StatusSession | undefined {
  return sessions(state).find((one) => one.title === title);
}

/** The project named `name`, if the daemon has one. */
export function project(name: string, state: Status = status()): StatusProject | undefined {
  return state.projects?.find((one) => one.name === name);
}

/** Waits until `read` gives something truthy, and returns it. */
export async function until<T>(
  read: () => T | undefined | null | false | Promise<T | undefined | null | false>,
  what: string,
  timeout = 20_000,
): Promise<T> {
  let got: T | undefined | null | false;
  await browser.waitUntil(
    async () => {
      got = await read();
      return Boolean(got);
    },
    { timeout, interval: 100, timeoutMsg: `waited ${timeout / 1000} s for ${what}` },
  );
  return got as T;
}

/** Waits until the daemon's state satisfies `wanted`. */
export function untilStatus(wanted: (state: Status) => boolean, what: string, timeout?: number) {
  return until(
    () => {
      const state = status();
      return wanted(state) ? state : null;
    },
    what,
    timeout,
  );
}

/** Waits until the session titled `title` has `wanted` status (`needs_you`, `working`, `finished`…). */
export function untilSession(title: string, wanted: string, timeout?: number) {
  return until(
    () => {
      const found = session(title);
      return found?.status === wanted ? found : null;
    },
    `${title} to be ${wanted}`,
    timeout,
  );
}

/** The person's settings, as Muster wrote them (settings.toml); empty if none. */
export function settingsToml(): string {
  const file = join(sandbox().config, "settings.toml");
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}

/** The daemon's log, for a failure's message. */
export function daemonLog(): string {
  const file = join(sandbox().runtime, "daemon.log");
  return existsSync(file) ? readFileSync(file, "utf8") : "";
}
