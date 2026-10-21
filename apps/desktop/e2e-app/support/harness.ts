import { type ChildProcess, execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, openSync } from "node:fs";
import { join, resolve } from "node:path";
import { browser } from "@wdio/globals";
import { makeSandbox, removeSandbox, type Sandbox, sandboxEnv } from "./sandbox";

// The real app over a sandbox of its own, one per spec file: wdio.conf.ts
// calls `startHarness` before the worker's WebDriver session begins and
// `stopHarness` after it ends. The app is started here, not by a service:
// so each spec file gets a fresh daemon, and a test can start the app
// again over the same daemon (`restartApp`).

const here = resolve(import.meta.dirname, "..", "..");

/** The app under test, as `pnpm e2e:app` builds it. */
export const APP = join(here, "src-tauri/target/debug/muster-desktop");

/** Where a failed test's screenshot and the app's own output go. */
export const OUTPUT = join(here, "test-results/e2e-app");

/**
 * The embedded WebDriver server's port: each worktree its own, so branches
 * tested side by side never reach each other's app.
 */
export const PORT =
  4445 + (Number.parseInt(createHash("sha1").update(here).digest("hex").slice(0, 4), 16) % 1000);

let current: Sandbox | null = null;
let app: ChildProcess | null = null;

/** This spec file's sandbox. */
export function sandbox(): Sandbox {
  // Kept in the worker's environment too: the config and the specs may be
  // loaded as modules of their own.
  current ??= process.env.MUSTER_E2E_SANDBOX ? JSON.parse(process.env.MUSTER_E2E_SANDBOX) : null;
  if (!current) throw new Error("no sandbox: the harness has not started");
  return current;
}

async function answering(timeout: number): Promise<boolean> {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`http://127.0.0.1:${PORT}/status`);
      if (response.ok) return true;
    } catch {
      // Not yet listening.
    }
    await new Promise((done) => setTimeout(done, 100));
  }
  return false;
}

/** Whether a process of this worktree's app still runs. */
export function appRuns(): boolean {
  try {
    execFileSync("pgrep", ["-f", APP], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** Starts the app over the sandbox, and waits for its WebDriver server. */
export async function startApp(extra: Record<string, string> = {}): Promise<void> {
  const box = sandbox();
  mkdirSync(OUTPUT, { recursive: true });
  const log = openSync(join(box.root, "app.log"), "a");
  app = spawn(APP, [], {
    env: { ...sandboxEnv(box), TAURI_WEBDRIVER_PORT: String(PORT), ...extra },
    stdio: ["ignore", log, log],
  });
  if (!(await answering(30_000))) {
    throw new Error(`the app's WebDriver server did not answer on port ${PORT}`);
  }
}

/** Ends the app, and waits until it is gone. The daemon runs on (`PRD-09`). */
export async function stopApp(): Promise<void> {
  if (app) app.kill("SIGTERM");
  else if (appRuns()) execFileSync("pkill", ["-f", APP], { stdio: "ignore" });
  app = null;
  for (let waited = 0; appRuns() && waited < 50; waited += 1) {
    await new Promise((done) => setTimeout(done, 100));
  }
  if (appRuns()) execFileSync("pkill", ["-9", "-f", APP], { stdio: "ignore" });
}

/** Ends the app and starts it again over the same sandbox; the WebDriver session with it. */
export async function restartApp(extra: Record<string, string> = {}): Promise<void> {
  await stopApp();
  await startApp(extra);
  await browser.reloadSession();
}

/** Makes this spec file's sandbox and starts the app over it. */
export async function startHarness(): Promise<void> {
  // An app left by a run that was cut short would hold the port.
  if (appRuns()) execFileSync("pkill", ["-9", "-f", APP], { stdio: "ignore" });
  current = makeSandbox();
  process.env.MUSTER_E2E_SANDBOX = JSON.stringify(current);
  await startApp();
}

/**
 * Ends the app before its daemon (an app still running would start it
 * again), then stops the daemon and removes the sandbox.
 */
export async function stopHarness(): Promise<void> {
  await stopApp();
  if (process.env.MUSTER_E2E_SANDBOX) removeSandbox(sandbox());
  current = null;
  delete process.env.MUSTER_E2E_SANDBOX;
}
