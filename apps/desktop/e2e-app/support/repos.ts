import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { sandbox } from "./harness";
import { sandboxEnv } from "./sandbox";

// Git repositories for the tests, with commits, branches and changes not
// yet committed, and the stand-in `gh`'s tickets (support/stand-ins.ts).

/** Runs git in `cwd` with the sandbox's identity; what it printed. */
export function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, env: sandboxEnv(sandbox()), encoding: "utf8" }).trim();
}

/**
 * A repository named `name` with one commit on `main`, hosted on GitHub as
 * far as Muster can tell (a `github` remote never fetched from), so its
 * tickets are the stand-in `gh`'s. Its path, as git reads it.
 */
export function repository(name: string): string {
  const root = join(sandbox().root, "repos", name);
  mkdirSync(root, { recursive: true });
  git(root, "init", "--quiet", "--initial-branch", "main");
  writeFileSync(join(root, "README.md"), "test\n");
  git(root, "add", "README.md");
  git(root, "commit", "--quiet", "--message", "first");
  git(root, "remote", "add", "github", `https://github.com/o/${name}.git`);
  return realpathSync(root);
}

/** Writes `files` (path to text) in `cwd` and commits them as `message`. */
export function commit(cwd: string, message: string, files: Record<string, string>): void {
  for (const [file, text] of Object.entries(files)) write(cwd, file, text);
  git(cwd, "add", "--", ...Object.keys(files));
  git(cwd, "commit", "--quiet", "--message", message);
}

/** Writes `text` to `file` in `cwd`, a change not committed. */
export function write(cwd: string, file: string, text: string): void {
  const path = join(cwd, file);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, text);
}

/** Removes `file` in `cwd`, a change not committed. */
export function remove(cwd: string, file: string): void {
  rmSync(join(cwd, file));
}

/** The text of `file` in `cwd`, or null if there is none. */
export function read(cwd: string, file: string): string | null {
  const path = join(cwd, file);
  return existsSync(path) ? readFileSync(path, "utf8") : null;
}

/** A branch made at the current commit, without checking it out. */
export function branch(cwd: string, name: string): void {
  git(cwd, "branch", name);
}

/** The repository's branches. */
export function branches(cwd: string): string[] {
  return git(cwd, "branch", "--format=%(refname:short)").split("\n").filter(Boolean);
}

/** The subjects of the commits on `ref`, newest first. */
export function log(cwd: string, ref = "HEAD"): string[] {
  return git(cwd, "log", "--format=%s", ref).split("\n").filter(Boolean);
}

/** `git status --porcelain`: the working tree's changes, one per line. */
export function changes(cwd: string): string[] {
  return git(cwd, "status", "--porcelain").split("\n").filter(Boolean);
}

/** One of the stand-in tracker's tickets. */
export interface Ticket {
  number: number;
  title: string;
  body?: string;
  labels?: string[];
  author?: string;
  /** Whose it is; the person's own (`@me`) where they are the assignee. */
  mine?: boolean;
}

function asGh(ticket: Ticket, repo: string) {
  return {
    number: ticket.number,
    title: ticket.title,
    url: `https://github.com/o/${repo}/issues/${ticket.number}`,
    labels: (ticket.labels ?? []).map((name) => ({ name })),
    author: { login: ticket.author ?? "someone" },
  };
}

/**
 * The stand-in `gh` answers with `tickets`, for any repository: `issue
 * list` lists them, `issue view N` shows each with its text.
 */
export function tickets(list: Ticket[], repo = "app"): void {
  const dir = sandbox().gh;
  writeFileSync(join(dir, "issues.json"), JSON.stringify(list.map((one) => asGh(one, repo))));
  const mine = list.filter((one) => one.mine).map((one) => asGh(one, repo));
  writeFileSync(join(dir, "mine.json"), JSON.stringify(mine));
  for (const ticket of list) {
    writeFileSync(
      join(dir, `issue-${ticket.number}.json`),
      JSON.stringify({ ...asGh(ticket, repo), body: ticket.body ?? "" }),
    );
  }
}

/** Makes the stand-in tracker out of reach (`true`) or back. */
export function trackerDown(down: boolean): void {
  const flag = join(sandbox().gh, "gh-down");
  if (down) writeFileSync(flag, "");
  else rmSync(flag, { force: true });
}

/** The calls the stand-in `gh` was given, one per line. */
export function ghCalls(): string[] {
  const file = join(sandbox().gh, "calls");
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean) : [];
}

/** The tickets the stand-in `gh` was asked to close. */
export function ticketsClosed(): number[] {
  const file = join(sandbox().gh, "closed");
  return existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean).map(Number) : [];
}
