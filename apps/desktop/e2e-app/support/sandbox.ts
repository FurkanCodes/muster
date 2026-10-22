import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { targetDir } from "./cargo";
import { STAND_IN_AGENT, STAND_IN_COPY, STAND_IN_GH, STAND_IN_MUSTER } from "./stand-ins";

// A sandbox of its own for each spec file (wdio.conf.ts makes it before the
// app starts and removes it after): a fresh daemon, its own folders, and
// stand-ins on PATH for every program Muster runs that would reach
// something real (the agents, `gh`, the clipboard). Nothing reaches the
// person's own Muster: its runtime folder is under /tmp (a socket's path
// must stay short), the rest under the system's temporary folder, and HOME
// is the sandbox's own, for the window's WebKit storage too.

const root = resolve(import.meta.dirname, "../../../..");

/** Muster itself, the core workspace's debug build, unless MUSTER_E2E_MUSTER names one. */
export const MUSTER = process.env.MUSTER_E2E_MUSTER ?? join(targetDir(root), "debug/muster");

/** The agents Muster knows, each played by the stand-in. */
export const AGENTS = ["claude", "codex", "gemini", "opencode", "copilot"] as const;
export type AgentKind = (typeof AGENTS)[number];

export interface Sandbox {
  /** The sandbox's own folder: repositories, stand-ins' records. */
  readonly root: string;
  /** Muster's runtime folder: the daemon's socket and daemon.log. */
  readonly runtime: string;
  /** Where Muster keeps the person's settings (settings.toml). */
  readonly config: string;
  readonly data: string;
  readonly worktrees: string;
  /** The stand-in agents' records, one folder per session. */
  readonly agents: string;
  /** The stand-in `gh`'s fixtures and the calls it was given. */
  readonly gh: string;
  /** What the app did that would reach outside it in a real build: opened, heard. */
  readonly record: string;
  /** Every variable the app, its daemon and `muster` here are given. */
  readonly env: Readonly<Record<string, string>>;
}

function write(path: string, text: string, mode = 0o644): void {
  writeFileSync(path, text);
  chmodSync(path, mode);
}

/** Bakes `values` into a stand-in's script: the app's env is fixed when it starts. */
function fill(script: string, values: Record<string, string>): string {
  return script.replace(/@(\w+)@/g, (_, name: string) => values[name] ?? "");
}

export function makeSandbox(): Sandbox {
  const sandbox = mkdtempSync(join(tmpdir(), "muster-e2e-app-"));
  const runtime = mkdtempSync("/tmp/mde");
  const at = (...parts: string[]) => join(sandbox, ...parts);
  const folders = {
    bin: at("bin"),
    home: at("home"),
    agents: at("agents"),
    gh: at("gh"),
    record: at("record"),
    repos: at("repos"),
    claude: at("claude-config"),
    clipboard: at("clipboard"),
  };
  for (const folder of Object.values(folders)) mkdirSync(folder, { recursive: true });
  const values = {
    AGENTS: folders.agents,
    GH: folders.gh,
    CLIPBOARD: folders.clipboard,
    MUSTER: MUSTER,
    RECORD: folders.record,
  };
  for (const agent of AGENTS) write(join(folders.bin, agent), fill(STAND_IN_AGENT, values), 0o755);
  write(join(folders.bin, "gh"), fill(STAND_IN_GH, values), 0o755);
  write(join(folders.bin, "pbcopy"), fill(STAND_IN_COPY, values), 0o755);
  write(join(folders.bin, "muster"), fill(STAND_IN_MUSTER, values), 0o755);
  write(join(folders.gh, "issues.json"), "[]\n");
  write(join(folders.gh, "mine.json"), "[]\n");

  const env: Record<string, string> = {
    HOME: folders.home,
    // The home the Mac's own frameworks read (NSHomeDirectory), which
    // ignores HOME: WebKit's storage and caches of the window go in the
    // sandbox, never in the person's ~/Library/WebKit/muster-desktop,
    // which every worktree's app would share.
    CFFIXED_USER_HOME: folders.home,
    MUSTER_RUNTIME_DIR: runtime,
    MUSTER_DATA_DIR: at("data"),
    MUSTER_CONFIG_DIR: at("config"),
    MUSTER_WORKTREES_DIR: at("worktrees"),
    // The app starts its daemon through the stand-in, which a test can
    // make refuse, and which logs each start.
    MUSTER_BIN: join(folders.bin, "muster"),
    MUSTER_E2E_RECORD: folders.record,
    PATH: `${folders.bin}:/usr/bin:/bin:/usr/sbin:/sbin`,
    // Sessions run a plain, predictable shell, and the login shell the app
    // reads the person's PATH from reads nothing of the person's.
    SHELL: "/bin/sh",
    // `--detach` starts the daemon directly, as over SSH, rather than
    // through a launchd job of the person's.
    SSH_CONNECTION: "muster-e2e-app",
    MUSTER_NOTCH: "off",
    MUSTER_KEYCHAIN: "off",
    MUSTER_LOG: "debug",
    CLAUDE_CONFIG_DIR: folders.claude,
    // The secret the fixture ticket plugin asks for (support/ticket-plugin.ts).
    FIXTURE_TOKEN: "fixture-token",
    CLAUDE_CODE_OAUTH_TOKEN: "",
    ANTHROPIC_API_KEY: "",
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
    GIT_AUTHOR_NAME: "Test",
    GIT_AUTHOR_EMAIL: "test@example.com",
    GIT_COMMITTER_NAME: "Test",
    GIT_COMMITTER_EMAIL: "test@example.com",
    LANG: "en_US.UTF-8",
    TMPDIR: process.env.TMPDIR ?? "/tmp",
  };
  return {
    root: sandbox,
    runtime,
    config: env.MUSTER_CONFIG_DIR as string,
    data: env.MUSTER_DATA_DIR as string,
    worktrees: env.MUSTER_WORKTREES_DIR as string,
    agents: folders.agents,
    gh: folders.gh,
    record: folders.record,
    env,
  };
}

/** What every process the tests start is given: the sandbox's variables, nothing of the person's Muster. */
export function sandboxEnv(sandbox: Sandbox): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = { ...process.env, ...sandbox.env };
  // Not inside the person's Muster session, whatever runs the tests: never
  // its session, nor its hooks' socket and token.
  delete env.MUSTER_SESSION;
  delete env.MUSTER_HOOK_SOCKET;
  delete env.MUSTER_HOOK_TOKEN;
  delete env.GEMINI_CLI_SYSTEM_DEFAULTS_PATH;
  delete env.OPENCODE_CONFIG_CONTENT;
  return env;
}

/** Stops whatever still runs from the sandbox (its daemon, its sessions) and removes it. */
export function removeSandbox(sandbox: Sandbox): void {
  const env = sandboxEnv(sandbox);
  let pid: number | undefined;
  try {
    const said = execFileSync(MUSTER, ["status", "--json"], { env, encoding: "utf8" });
    pid = (JSON.parse(said) as { daemon?: { pid: number } }).daemon?.pid;
  } catch {
    // None answers.
  }
  try {
    execFileSync(MUSTER, ["daemon", "stop"], { env, stdio: "ignore" });
  } catch {
    // None was running.
  }
  // The daemon writes its last words as it goes: its folders are removed
  // once it has gone, and one that outlives its stop is ended.
  if (pid) {
    for (let waited = 0; alive(pid) && waited < 50; waited += 1) {
      execFileSync("sleep", ["0.1"]);
    }
    if (alive(pid)) process.kill(pid, "SIGKILL");
  }
  for (const folder of [sandbox.root, sandbox.runtime]) {
    try {
      execFileSync("pkill", ["-f", folder], { stdio: "ignore" });
    } catch {
      // Nothing left.
    }
  }
  for (const folder of [sandbox.root, sandbox.runtime]) {
    rmSync(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  }
}

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/**
 * Ends any daemon of this worktree's `muster` still running: one a worker
 * that crashed never stopped. The person's own daemon is another program.
 */
export function endStrayDaemons(): void {
  try {
    execFileSync("pkill", ["-f", `${MUSTER} daemon`], { stdio: "ignore" });
  } catch {
    // None.
  }
}
