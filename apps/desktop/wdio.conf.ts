import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { browser } from "@wdio/globals";
import { APP, appRuns, OUTPUT, PORT, startHarness, stopHarness } from "./e2e-app/support/harness";
import { MUSTER } from "./e2e-app/support/sandbox";
import { frontWindow, theWindow, toFront } from "./e2e-app/support/window";

// The real app over a real daemon of its own (ARCHITECTURE.md §5c, §13):
// the app built with `--features e2e` (`pnpm e2e:app` builds it), its
// embedded WebDriver server (tauri-plugin-wdio-webdriver) driven by
// WebdriverIO as a plain WebDriver server.
//
// Each spec file runs in a worker of its own, over a sandbox of its own
// (e2e-app/support/harness.ts): before its session the worker makes the
// sandbox and starts the app over it; after, it ends the app, stops the
// daemon the app started and removes the sandbox. Nothing here reaches the
// person's own Muster.

const root = resolve(import.meta.dirname, "../..");

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./e2e-app/**/*.spec.ts"],
  maxInstances: 1,
  hostname: "127.0.0.1",
  port: PORT,
  // The embedded server drives the app's one webview whatever is asked.
  capabilities: [{ browserName: "tauri" }],
  logLevel: "warn",
  waitforTimeout: 20_000,
  connectionRetryTimeout: 90_000,
  connectionRetryCount: 1,
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 120_000 },
  reporters: ["spec"],

  /** `muster` itself, from the core workspace, unless MUSTER_E2E_MUSTER names one. */
  onPrepare() {
    if (MUSTER === join(root, "target/debug/muster")) {
      execFileSync("cargo", ["build", "--locked", "--quiet", "-p", "muster"], {
        cwd: root,
        stdio: "inherit",
      });
    }
    mkdirSync(OUTPUT, { recursive: true });
  },

  async beforeSession() {
    await startHarness();
  },

  async before() {
    await theWindow();
    await frontWindow(PORT);
  },

  /** Each test starts with the window in front, where another's cannot cover it all. */
  async beforeTest() {
    await toFront();
  },

  /** A failed test leaves a picture of the window, named for the test. */
  async afterTest(test, _context, { passed }) {
    if (passed) return;
    const name = `${test.parent} ${test.title}`.replace(/[^\w-]+/g, "-").slice(0, 120);
    try {
      await browser.saveScreenshot(join(OUTPUT, `${name}.png`));
    } catch {
      // The app had already gone.
    }
  },

  async afterSession() {
    await stopHarness();
  },

  /** Nothing of this worktree's app outlives the run, however it ended. */
  onComplete() {
    if (appRuns()) execFileSync("pkill", ["-9", "-f", APP], { stdio: "ignore" });
  },
};
