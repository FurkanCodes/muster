// A throwaway world for recording Muster's terminal UI: its own daemon,
// data, settings and worktrees in a folder of its own, git repositories to
// work in, and stand-ins for Claude Code and the GitHub CLI on its PATH, as
// the end-to-end tests have (crates/muster/tests/support). The stand-in
// agent prints what the recorder gives it and reports to the daemon only
// when the recorder posts a hook for it, the way Claude Code's hooks do.
// Nothing reaches a real agent, tracker or the person's own Muster.

import { execFileSync, spawn } from "node:child_process";
import { appendFileSync, chmodSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { join } from "node:path";

/** Stands in for Claude Code: notes where its hooks go, then shows its screen file. */
const CLAUDE = `#!/bin/sh
for arg in "$@"; do [ "$arg" = "-p" ] && exit 0; done
dir="$WORLD_AGENTS/$MUSTER_SESSION"
mkdir -p "$dir"
printf '%s\\n%s\\n' "$MUSTER_HOOK_SOCKET" "$MUSTER_HOOK_TOKEN" > "$dir/env"
pwd > "$dir/folder"
: >> "$dir/screen"
# What is typed reaches it without being echoed, as in a real agent.
stty -echo 2>/dev/null
exec tail -n +1 -f "$dir/screen"
`;

/** Stands in for the GitHub CLI: its issues from fixture files, as the tests' does. */
const GH = `#!/bin/sh
dir="$WORLD_GH"
# Each repository's own issues, by its name on GitHub.
name=$(basename "$(git remote get-url origin 2>/dev/null)" .git)
case "$1 $2" in
  "issue list") if [ -f "$dir/issues-$name.json" ]; then cat "$dir/issues-$name.json"; else echo "[]"; fi ;;
  "issue view") cat "$dir/issue-$3.json" ;;
  "pr list") if [ -f "$dir/pr-$(git branch --show-current 2>/dev/null | tr / -).json" ]; then cat "$dir/pr-$(git branch --show-current | tr / -).json"; else echo "[]"; fi ;;
  *) echo "[]" ;;
esac
`;

/** Keeps what it was given, so nothing reaches the person's clipboard. */
const COPY = `#!/bin/sh
cat > /dev/null
`;

export function makeWorld() {
  // One plain, short path, as paths show on screen: the daemon's sockets
  // live in it too, and a socket's path is limited. Its real path, as the
  // daemon writes paths (/tmp is a link on a Mac). A world left by a run
  // that stopped half way is cleared first.
  rmSync("/tmp/muster-demo", { recursive: true, force: true });
  mkdirSync("/tmp/muster-demo");
  const root = realpathSync("/tmp/muster-demo");
  const dir = (name) => {
    const path = join(root, name);
    mkdirSync(path, { recursive: true });
    return path;
  };
  const bin = dir("bin");
  const agents = dir("agents");
  const gh = dir("gh");
  const home = dir("home");
  for (const [name, script] of [["claude", CLAUDE], ["gh", GH], ["pbcopy", COPY]]) {
    writeFileSync(join(bin, name), script);
    chmodSync(join(bin, name), 0o755);
  }
  const env = {
    PATH: `${bin}:${process.env.PATH}`,
    HOME: home,
    USER: "you",
    LANG: "en_US.UTF-8",
    TERM: "xterm-256color",
    COLORTERM: "truecolor",
    SHELL: "/bin/zsh",
    MUSTER_RUNTIME_DIR: dir("run"),
    MUSTER_DATA_DIR: dir("data"),
    MUSTER_WORKTREES_DIR: dir("worktrees"),
    MUSTER_CONFIG_DIR: dir("config"),
    // Nothing on the person's screen or in their Keychain.
    MUSTER_NOTIFY: "off",
    MUSTER_NOTCH: "off",
    MUSTER_KEYCHAIN: "off",
    CLAUDE_CONFIG_DIR: dir("claude-config"),
    WORLD_AGENTS: agents,
    WORLD_GH: gh,
    GIT_AUTHOR_NAME: "You",
    GIT_AUTHOR_EMAIL: "you@example.com",
    GIT_COMMITTER_NAME: "You",
    GIT_COMMITTER_EMAIL: "you@example.com",
    GIT_CONFIG_NOSYSTEM: "1",
  };
  let daemon = null;

  const git = (cwd, ...args) => execFileSync("git", args, { cwd, env, stdio: "pipe" });

  return {
    root,
    env,
    gh,

    /** `muster` with `args`, in `cwd`; what it printed. */
    muster(args, cwd = home) {
      return execFileSync("muster", args, { cwd, env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
    },

    /** A git repository of `files` in the world's code folder, hosted on GitHub as far as Muster can tell. */
    repository(name, files) {
      const path = join(dir("code"), name);
      mkdirSync(path, { recursive: true });
      git(path, "init", "--quiet", "--initial-branch", "main");
      for (const [file, text] of Object.entries(files)) {
        mkdirSync(join(path, file, ".."), { recursive: true });
        writeFileSync(join(path, file), text);
      }
      git(path, "add", "--all");
      git(path, "commit", "--quiet", "--message", "First commit");
      git(path, "remote", "add", "origin", `https://github.com/you/${name}.git`);
      return path;
    },

    git,

    async startDaemon() {
      daemon = spawn("muster", ["daemon"], { env, stdio: ["ignore", "ignore", "ignore"], detached: true });
      for (let tries = 0; tries < 100; tries++) {
        try {
          execFileSync("muster", ["status"], { env, stdio: "ignore" });
          return;
        } catch {
          await new Promise((done) => setTimeout(done, 100));
        }
      }
      throw new Error("the world's daemon did not start");
    },

    /** Every session, workspace and project, as `muster list --all --json` says. */
    list() {
      return JSON.parse(this.muster(["list", "--all", "--json"]));
    },

    /** The session whose title starts with `title`. */
    session(title) {
      const found = this.list().sessions.find((session) => session.title.startsWith(title));
      if (!found) throw new Error(`no session ${title}`);
      return found;
    },

    /** Waits until the stand-in started for `id` has said where its hooks go. */
    async started(id) {
      for (let tries = 0; tries < 100; tries++) {
        if (existsSync(join(agents, id, "env"))) return join(agents, id);
        await new Promise((done) => setTimeout(done, 100));
      }
      throw new Error(`the agent of ${id} never started`);
    },

    /** Posts a Claude Code hook for session `id`, as its hooks would. */
    async hook(id, payload) {
      const [socket, token] = readFileSync(join(agents, id, "env"), "utf8").split("\n");
      const body = JSON.stringify(payload);
      await new Promise((done, fail) => {
        const sent = request(
          {
            socketPath: socket,
            path: "/hooks/claude",
            method: "POST",
            headers: { authorization: `Bearer ${token}`, "content-type": "application/json", "content-length": Buffer.byteLength(body) },
          },
          (response) => {
            response.resume();
            response.on("end", done);
          },
        );
        sent.on("error", fail);
        sent.end(body);
      });
    },

    /** Prints `text` in session `id`'s terminal, as its agent would. */
    show(id, text) {
      appendFileSync(join(agents, id, "screen"), text);
    },

    /**
     * A turn of session `id`'s agent, as Claude Code's hooks tell it: the
     * prompt submitted, `files` written in its folder (paths from there),
     * then the turn's end.
     */
    async turn(id, files, { end = true } = {}) {
      await this.hook(id, { hook_event_name: "UserPromptSubmit", session_id: id });
      const folder = this.folder(id);
      for (const [file, text] of Object.entries(files)) {
        mkdirSync(join(folder, file, ".."), { recursive: true });
        writeFileSync(join(folder, file), text);
      }
      if (end) {
        await new Promise((done) => setTimeout(done, 300));
        await this.hook(id, { hook_event_name: "Stop", session_id: id });
      }
    },

    /** The folder session `id`'s agent works in. */
    folder(id) {
      return readFileSync(join(agents, id, "folder"), "utf8").trim();
    },

    stop() {
      try {
        execFileSync("muster", ["daemon", "stop"], { env, stdio: "ignore" });
      } catch {}
      if (daemon) {
        try {
          process.kill(-daemon.pid);
        } catch {}
      }
      rmSync(root, { recursive: true, force: true });
    },
  };
}
