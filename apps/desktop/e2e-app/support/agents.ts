import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { dirname, join } from "node:path";
import { sandbox } from "./harness";
import { session as findSession, muster, until, untilSession } from "./muster";

// The stand-in agents (support/stand-ins.ts), made to ask, work and finish
// from outside, as their real agent tells Muster: Claude Code by its hooks,
// posted to the socket and token it was started with
// (crates/muster-daemon/src/harness/claude.rs), Codex by its window title
// (codex.rs `read_title`). What the person sees is written to the agent's
// own terminal, and the files it changes in its own folder.

/** One start of a stand-in agent, as it recorded it. */
export interface Launch {
  /** The agent it was started as: `claude`, `codex`… */
  readonly agent: string;
  readonly socket: string;
  readonly token: string;
  /** Its terminal, written to as the agent writes to its screen. */
  readonly tty: string;
  /** The folder it works in: the worktree. */
  readonly cwd: string;
  /** The id Claude Code is given (`--session-id`), which its hooks carry. */
  readonly id: string;
  readonly pid: number;
  readonly args: readonly string[];
}

/** How the stand-in was started for the session `id`, oldest first. */
export function launches(id: string): Launch[] {
  const dir = join(sandbox().agents, id);
  if (!existsSync(dir)) return [];
  const found: Launch[] = [];
  for (let n = 1; existsSync(join(dir, `launch-${n}.env`)); n += 1) {
    const [agent, socket, token, tty, cwd, launchId, pid] = readFileSync(
      join(dir, `launch-${n}.env`),
      "utf8",
    ).split("\n");
    const args = readFileSync(join(dir, `launch-${n}.args`), "utf8").split("\0");
    args.pop();
    found.push({
      agent: agent ?? "",
      socket: socket ?? "",
      token: token ?? "",
      tty: tty ?? "",
      cwd: cwd ?? "",
      id: launchId ?? "",
      pid: Number(pid),
      args,
    });
  }
  return found;
}

/** The newest start of the session titled `title`, once it has started. */
export async function agentOf(title: string): Promise<Launch> {
  return until(() => {
    const found = findSession(title);
    return found ? launches(found.id).at(-1) : undefined;
  }, `the stand-in agent of ${title} to start`);
}

/**
 * Starts a session of `agent` in `project` (its checkout, or the worktree
 * on `branch`) as the person's terminal would, and waits for its agent.
 */
export async function startAgent(
  project: string,
  title: string,
  { agent = "claude", branch }: { agent?: string; branch?: string } = {},
): Promise<Launch> {
  // A worktree is named by its branch; the project's own checkout by its name.
  muster("session", "start", branch ?? project, "--harness", agent, "--title", title);
  return agentOf(title);
}

/** Posts one of Claude Code's hooks for `launch`, as its hook command does. */
export function hook(launch: Launch, event: string, extra: Record<string, unknown> = {}) {
  const body = JSON.stringify({
    hook_event_name: event,
    session_id: launch.id,
    cwd: launch.cwd,
    ...extra,
  });
  return new Promise<void>((done, failed) => {
    const sent = request(
      {
        socketPath: launch.socket,
        path: "/hooks/claude",
        method: "POST",
        headers: {
          Authorization: `Bearer ${launch.token}`,
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
        },
      },
      (response) => {
        response.resume();
        if ((response.statusCode ?? 500) < 300) done();
        else failed(new Error(`the ${event} hook was answered ${response.statusCode}`));
      },
    );
    sent.on("error", failed);
    sent.end(body);
  });
}

/** Writes `text` to the agent's terminal, as the agent prints to its screen. */
export function show(launch: Launch, text: string): void {
  writeFileSync(launch.tty, text);
}

/** Sets the agent's window title, as Codex does to say how it stands. */
export function title(launch: Launch, text: string): void {
  show(launch, `\x1b]0;${text}\x07`);
}

/**
 * The agent asks the person: its screen cleared to just `question`, then
 * Muster told it waits (Claude Code's permission dialog, Codex's title).
 */
export async function ask(launch: Launch, question: string): Promise<void> {
  show(launch, `\x1b[2J\x1b[H${question.replaceAll("\n", "\r\n")}\r\n`);
  if (launch.agent === "claude") {
    await hook(launch, "PermissionRequest", { tool_name: "Bash", tool_input: { command: "ls" } });
  } else title(launch, "[ ! ] Action Required");
}

/** The agent takes a prompt and works on it. */
export async function work(launch: Launch, prompt = "go on"): Promise<void> {
  if (launch.agent === "claude") await hook(launch, "UserPromptSubmit", { prompt });
  else title(launch, "⠋ Working");
}

/** The agent's turn ends. */
export async function finish(launch: Launch): Promise<void> {
  if (launch.agent === "claude") await hook(launch, "Stop", { stop_hook_active: false });
  else title(launch, "Ready");
}

/** The agent writes `text` to `file` in its folder, as an edit it makes. */
export function change(launch: Launch, file: string, text: string): void {
  const path = join(launch.cwd, file);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

/** A session of a stand-in Claude Code titled `title`, asking `question`: in Needs you. */
export async function askingSession(
  project: string,
  title: string,
  question: string,
  options: { agent?: string; branch?: string } = {},
): Promise<Launch> {
  const launch = await startAgent(project, title, options);
  await ask(launch, question);
  await untilSession(title, "needs_you");
  return launch;
}

/** The files in the agent's folder, for asserting what it changed. */
export function filesOf(launch: Launch): string[] {
  return readdirSync(launch.cwd);
}
