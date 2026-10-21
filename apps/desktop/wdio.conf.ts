import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { TauriCapabilities } from "@wdio/tauri-service";

// The real app over a real daemon of its own (ARCHITECTURE.md §5c, §13):
// the app built with `--features e2e` (`pnpm e2e:app` builds it), its
// embedded WebDriver server (tauri-plugin-wdio-webdriver) driven by
// WebdriverIO through @wdio/tauri-service.
//
// Nothing here reaches the person's own Muster: the app, the daemon it
// starts and the `muster` the specs run all read the folders set below, a
// runtime folder of their own under /tmp (its socket's path must stay
// short) and the rest in a temporary folder. The sandbox is made once, by
// the launcher, before it starts the app; the workers that run the specs
// inherit its variables and find it there.

const here = import.meta.dirname;
const root = resolve(here, "../..");

/** Stands in for an agent: a plain shell that, as it starts, says in its
 * title that it waits on the person (as Codex does) and asks something, so
 * the window shows it in Needs you, then takes what is typed. */
const STAND_IN = `#!/bin/sh
printf '\\033]0;[ ! ] Action Required\\007'
echo 'Shall the stand-in go on?'
exec /bin/sh
`;

function git(cwd: string, ...args: string[]): void {
  execFileSync("git", args, {
    cwd,
    stdio: "ignore",
    env: {
      ...process.env,
      GIT_AUTHOR_NAME: "Test",
      GIT_AUTHOR_EMAIL: "test@example.com",
      GIT_COMMITTER_NAME: "Test",
      GIT_COMMITTER_EMAIL: "test@example.com",
    },
  });
}

function makeSandbox(): void {
  const sandbox = mkdtempSync(join(tmpdir(), "muster-e2e-app-"));
  const runtime = mkdtempSync("/tmp/mde");
  const bin = join(sandbox, "bin");
  mkdirSync(bin);
  // Codex, as the daemon finds it on PATH: the stand-in.
  writeFileSync(join(bin, "codex"), STAND_IN);
  chmodSync(join(bin, "codex"), 0o755);
  const repo = join(sandbox, "repos", "app");
  mkdirSync(repo, { recursive: true });
  Object.assign(process.env, {
    MUSTER_E2E_SANDBOX: sandbox,
    MUSTER_E2E_REPO: repo,
    MUSTER_RUNTIME_DIR: runtime,
    MUSTER_DATA_DIR: join(sandbox, "data"),
    MUSTER_CONFIG_DIR: join(sandbox, "config"),
    MUSTER_WORKTREES_DIR: join(sandbox, "worktrees"),
    MUSTER_BIN: process.env.MUSTER_BIN ?? join(root, "target/debug/muster"),
    PATH: `${bin}:${process.env.PATH ?? "/usr/bin:/bin"}`,
    // Sessions run a plain, predictable shell.
    SHELL: "/bin/sh",
    // `--detach` starts the daemon directly, as over SSH, rather than
    // through a launchd job of the person's.
    SSH_CONNECTION: "muster-e2e-app",
    MUSTER_NOTCH: "off",
    MUSTER_KEYCHAIN: "off",
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
  });
  // Not inside the person's Muster session, whatever runs the tests.
  delete process.env.MUSTER_SESSION;
  git(repo, "init", "--quiet", "--initial-branch", "main");
  writeFileSync(join(repo, "README.md"), "test\n");
  git(repo, "add", "README.md");
  git(repo, "commit", "--quiet", "--message", "first");
}

if (!process.env.MUSTER_E2E_SANDBOX) makeSandbox();

/** The app under test, as `pnpm e2e:app` builds it. */
const APP = resolve(here, "src-tauri/target/debug/muster-desktop");

/** Whether this worktree's app still runs. */
function appRuns(): boolean {
  try {
    execFileSync("pgrep", ["-f", APP], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

// Each worktree its own port, so branches tested side by side never reach
// each other's app.
const port =
  4445 + (Number.parseInt(createHash("sha1").update(here).digest("hex").slice(0, 4), 16) % 1000);

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./e2e-app/**/*.spec.ts"],
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application: APP },
    } satisfies TauriCapabilities as WebdriverIO.Capabilities,
  ],
  services: [["@wdio/tauri-service", { driverProvider: "embedded", embeddedPort: port }]],
  logLevel: "warn",
  waitforTimeout: 20_000,
  connectionRetryTimeout: 90_000,
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 90_000 },
  reporters: ["spec"],

  /** `muster` itself, from the core workspace, unless MUSTER_BIN names one. */
  onPrepare() {
    if (process.env.MUSTER_BIN === join(root, "target/debug/muster")) {
      execFileSync("cargo", ["build", "--locked", "--quiet", "-p", "muster"], {
        cwd: root,
        stdio: "inherit",
      });
    }
  },

  /**
   * The daemon the app started outlives it by design: stopped here. WDIO
   * runs this before the service ends the app, and an app still running
   * would start its daemon again: so the app goes first.
   */
  async onComplete() {
    if (appRuns()) execFileSync("pkill", ["-f", APP], { stdio: "ignore" });
    for (let waited = 0; appRuns() && waited < 50; waited += 1) {
      await new Promise((done) => setTimeout(done, 100));
    }
    const bin = process.env.MUSTER_BIN;
    try {
      if (bin) execFileSync(bin, ["daemon", "stop"], { stdio: "ignore" });
    } catch {
      // None was running.
    }
    for (const folder of [process.env.MUSTER_E2E_SANDBOX, process.env.MUSTER_RUNTIME_DIR]) {
      if (folder) rmSync(folder, { recursive: true, force: true });
    }
  },
};
