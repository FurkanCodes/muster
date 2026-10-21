// The commands the flows send, answered as the daemon answers them, with
// its refusals word for word (crates/muster-daemon/src/supervisor/
// projects.rs, worktrees.rs, sessions.rs, names.rs, tidy.rs, stacks.rs,
// changes.rs, issues.rs), and what git would say kept in `FlowWorld`.

import { agentOf } from "../../lib/agents";
import { BridgeFailure } from "../bridge";
import type { Changes } from "../generated/Changes";
import type { Command } from "../generated/Command";
import type { Folder } from "../generated/desktop/Folder";
import type { Patch } from "../generated/desktop/Patch";
import type { Issue } from "../generated/Issue";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { Workspace } from "../generated/Workspace";
import type { MockBridge } from "./bridge";
import { folderTree } from "./folders";
import { NOW, type Scenario, ulid } from "./scenario";

/** What the mock's git and file system say, which tests change. */
export class FlowWorld {
  /** Each folder's folders, by path (see folders.ts). */
  readonly folders: Record<string, Folder[]> = folderTree();
  /** Folders list_folders cannot read, and why. */
  readonly unreadable: Record<string, string> = {};
  /** What each workspace has changed, as WorkspaceChanges answers. */
  readonly changes: Record<string, Changes> = {};
  /** Each project's tickets, as ListIssues answers. */
  readonly issues: Record<string, Issue[]> = {};
  /** Holds sessions' exits after a stop, until `exitHeld`; the daemon's come a moment later. */
  holdExits = false;
  readonly held: string[] = [];
  next = 0;

  constructor(world: Scenario) {
    const commits = [
      { id: "a1b2c3d", subject: "Settle the resize on release" },
      { id: "b2c3d4e", subject: "Keep the panel width" },
      { id: "c3d4e5f", subject: "Test the resize under load" },
    ];
    for (const workspace of world.workspaces) {
      const worktree = workspace.kind.kind === "worktree";
      this.changes[workspace.id] = {
        base: "origin/main",
        uncommitted: [],
        commits: worktree ? commits : [],
        more_commits: 0,
        branch: [],
      };
    }
    // One worktree with work not committed, as git would find it.
    const dirty = this.changes[ulid(22)];
    if (dirty)
      dirty.uncommitted = [
        "app/build.gradle",
        "gradle.properties",
        "settings.gradle",
        "README.md",
      ].map((path) => ({ path, from: null, kind: "modified", lines: { added: 3, removed: 1 } }));
    this.issues[ulid(1)] = [
      issue("205", "Resize settles once on release"),
      issue("212", "Share the Gradle cache across worktrees"),
      issue("220", "Restart the daemon in place from Settings"),
    ];
  }
}

function issue(key: string, title: string): Issue {
  return {
    key,
    title,
    url: `https://github.com/FurkanCodes/muster/issues/${key}`,
    labels: [],
    author: null,
  };
}

function refuse(code: "bad_request" | "not_found" | "conflict", message: string): never {
  throw new BridgeFailure("refused", message, code);
}

/** `one_line`, then the cleaning `names::given` does: at most 80 characters. */
function given(title: string): string {
  const line = Array.from(title.replace(/[\n\r\t]/g, " "))
    .filter((c) => c >= " " && c !== "\u007f")
    .join("")
    .replace(/\s+/g, " ")
    .trim();
  return Array.from(line).slice(0, 80).join("").trimEnd();
}

/** The name a first prompt gives a chat (`names::from_prompt`): its first line, cut at a word to 60. */
function fromPrompt(prompt: string): string {
  const line =
    prompt
      .split(/\r?\n/)
      .find((l) => l.trim())
      ?.replace(/\s+/g, " ")
      .trim() ?? "";
  if (line.length <= 60) return line;
  const cut = line.slice(0, 59);
  return `${cut.slice(0, Math.max(cut.lastIndexOf(" "), 1))}…`;
}

const OWN = ["claude", "codex", "gemini", "opencode", "copilot"];

export function flowCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string") return undefined;
  const world = mock.world;
  const flows = mock.flows;
  const tell = (parts: Partial<Patch>) => mock.tell(parts);
  const id = () => ulid(5000 + ++flows.next);
  const project = (projectId: string) =>
    world.projects.find((p) => p.id === projectId) ??
    refuse("not_found", `no project ${projectId}`);
  const place = (workspaceId: string) =>
    world.workspaces.find((w) => w.id === workspaceId) ??
    refuse("not_found", `no workspace ${workspaceId}`);
  const session = (sessionId: string) =>
    world.sessions.find((s) => s.id === sessionId) ??
    refuse("not_found", `no session ${sessionId}`);
  const runs = (s: Session) => s.ended_at === null;
  const atWork = (workspaceId: string) =>
    world.sessions.find(
      (s) =>
        s.workspace === workspaceId &&
        runs(s) &&
        (s.status.state === "running" || s.status.state === "needs_you"),
    );
  const label = (harness: string) =>
    world.preferences.agents.find((a) => a.name === harness)?.label ?? agentOf(harness).name;
  const takesPrompt = (harness: string) =>
    OWN.includes(harness) ||
    (world.preferences.agents.find((a) => a.name === harness)?.takes_prompt ?? false);

  if ("AddProject" in command) {
    const root = command.AddProject.root.replace(/(.)\/+$/, "$1");
    if (!root.startsWith("/")) refuse("bad_request", `${root} is not an absolute path`);
    const folder = Object.values(flows.folders)
      .flat()
      .find((f) => f.path === root);
    if (!folder && !(root in flows.folders))
      refuse("bad_request", `cannot open ${root}: No such file or directory (os error 2)`);
    if (world.projects.some((p) => p.root === root))
      refuse("conflict", `${root} is already a project`);
    const added = {
      id: id(),
      name: command.AddProject.name?.trim() || root.split("/").at(-1) || root,
      root,
      created_at: NOW,
    };
    const checkout: Workspace = {
      id: id(),
      project: added.id,
      kind: { kind: "checkout", path: root, branch: folder?.git ? "main" : "" },
      created_at: NOW,
      pull_request: null,
      changes: null,
      timeline: null,
      clashes: [],
      stacked_on: null,
    };
    world.projects.push(added);
    world.workspaces.push(checkout);
    flows.changes[checkout.id] = {
      base: "origin/main",
      uncommitted: [],
      commits: [],
      more_commits: 0,
      branch: [],
    };
    tell({ projects: [added], workspaces: [checkout] });
    return { ProjectAdded: { id: added.id } };
  }

  if ("RemoveProject" in command) {
    const found = project(command.RemoveProject.project);
    const mine = world.workspaces.filter((w) => w.project === found.id);
    const worktrees = mine.filter((w) => w.kind.kind === "worktree").length;
    if (worktrees > 0)
      refuse("conflict", `${found.name} has ${worktrees} worktree(s); remove them first`);
    const running = world.sessions.filter(
      (s) => mine.some((w) => w.id === s.workspace) && runs(s),
    ).length;
    if (running > 0) refuse("conflict", `stop ${found.name}'s ${running} running session(s) first`);
    const sessions = world.sessions.filter((s) => mine.some((w) => w.id === s.workspace));
    world.projects = world.projects.filter((p) => p.id !== found.id);
    world.workspaces = world.workspaces.filter((w) => w.project !== found.id);
    world.sessions = world.sessions.filter((s) => !sessions.includes(s));
    tell({
      gone: {
        projects: [found.id],
        workspaces: mine.map((w) => w.id),
        sessions: sessions.map((s) => s.id),
      },
    });
    return "Done";
  }

  if ("CreateWorktree" in command) {
    const { branch, on } = command.CreateWorktree;
    const found = project(command.CreateWorktree.project);
    const checkout = world.workspaces.find(
      (w) => w.project === found.id && w.kind.kind === "checkout",
    );
    if (!checkout?.kind.branch)
      refuse(
        "bad_request",
        `${found.name} is a plain folder, not a git repository; worktrees need one`,
      );
    const made = id();
    const name = branch?.trim() || `muster/${made.slice(-6).toLowerCase()}`;
    if (/\s|\.\.|[~^:?*[\\]|^-|\/$|^\/|\.lock$|@\{/.test(name))
      refuse("bad_request", `"${name}" is not a valid branch name`);
    if (world.workspaces.some((w) => w.project === found.id && w.kind.branch === name))
      refuse("conflict", `${name} already has a worktree`);
    const parent = on ? place(on) : null;
    const workspace: Workspace = {
      id: made,
      project: found.id,
      kind: {
        kind: "worktree",
        path: `/Users/you/.muster/worktrees/${found.name}/${name}`,
        branch: name,
      },
      created_at: NOW,
      pull_request: null,
      changes: null,
      timeline: null,
      clashes: [],
      stacked_on: parent ? { branch: parent.kind.branch, at: "d4e5f6a" } : null,
    };
    world.workspaces.push(workspace);
    flows.changes[made] = {
      base: "origin/main",
      uncommitted: [],
      commits: [],
      more_commits: 0,
      branch: [],
    };
    tell({ workspaces: [workspace] });
    return { WorktreeCreated: { id: made } };
  }

  if ("RemoveWorktree" in command) {
    const { workspace, force } = command.RemoveWorktree;
    const found =
      world.workspaces.find((w) => w.id === workspace) ??
      refuse("not_found", `no worktree ${workspace}`);
    if (found.kind.kind === "checkout")
      refuse(
        "bad_request",
        "that is the project's own checkout, which Muster never removes; remove the project instead",
      );
    if (found.kind.kind === "container")
      refuse(
        "bad_request",
        "that is a run's container, which goes when the run ends; cancel the run instead",
      );
    const agents = world.sessions.filter(
      (s) =>
        s.workspace === workspace &&
        runs(s) &&
        s.harness !== "shell" &&
        !s.harness.startsWith("plugin:"),
    ).length;
    if (agents > 0) refuse("conflict", `stop the worktree's ${agents} running session(s) first`);
    if (!force && (flows.changes[workspace]?.uncommitted.length ?? 0) > 0)
      refuse(
        "conflict",
        `${found.kind.path} has uncommitted changes; remove it with force to discard them`,
      );
    const sessions = world.sessions.filter((s) => s.workspace === workspace).map((s) => s.id);
    world.workspaces = world.workspaces.filter((w) => w.id !== workspace);
    world.sessions = world.sessions.filter((s) => s.workspace !== workspace);
    tell({ gone: { projects: [], workspaces: [workspace], sessions } });
    return "Done";
  }

  if ("StartSession" in command) {
    const { workspace, harness, title, prompt } = command.StartSession;
    const known =
      harness === "shell" ||
      OWN.includes(harness) ||
      world.preferences.agents.some((a) => a.name === harness);
    if (!known)
      refuse(
        "bad_request",
        `no harness called "${harness}"; add an agent of your own as [agents.${harness}] in ${world.preferences.file}`,
      );
    const found = place(workspace);
    if (found.kind.kind === "container" && harness !== "shell")
      refuse(
        "bad_request",
        `${label(harness)} runs in a run's container only for the person taking the run over (\`muster run take-over\`); a shell can look around`,
      );
    const asked = prompt?.trim() ? prompt : null;
    if (asked && !takesPrompt(harness)) refuse("bad_request", `${label(harness)} takes no prompt`);
    const started: Session = {
      id: id(),
      workspace,
      harness,
      title: title?.trim() || (asked ? fromPrompt(asked) : label(harness)),
      status: asked ? { state: "running" } : { state: "fresh" },
      unseen: false,
      resume_id: null,
      created_at: NOW,
      ended_at: null,
      status_since: NOW,
      activity: null,
      spend: null,
      tidy: null,
      beside: null,
    };
    world.sessions.push(started);
    tell({ sessions: [started] });
    return { SessionStarted: { id: started.id } };
  }

  if ("StopSession" in command) {
    const found = world.sessions.find((s) => s.id === command.StopSession.session);
    if (!found || !runs(found))
      refuse("not_found", `${command.StopSession.session} is not running`);
    // The daemon says yes once the program is told; its end comes after.
    flows.held.push(found.id);
    if (!flows.holdExits) queueMicrotask(() => mock.exitHeld());
    return "Done";
  }

  if ("RemoveSession" in command) {
    const found = session(command.RemoveSession.session);
    if (runs(found)) refuse("conflict", `${found.title} is still running; stop it first`);
    mock.removeSession(found.id);
    return "Done";
  }

  if ("RestartSession" in command) {
    const found = session(command.RestartSession.session);
    if (runs(found)) refuse("conflict", `${found.title} is still running`);
    mock.updateSession(found.id, { ended_at: null });
    mock.setStatus(found.id, { state: "fresh" });
    return { SessionStarted: { id: found.id } };
  }

  if ("RenameSession" in command) {
    const found = session(command.RenameSession.session);
    const title = given(command.RenameSession.title);
    if (!title) refuse("bad_request", "a session's name cannot be empty");
    mock.updateSession(found.id, { title });
    mock.emitSession(found.id);
    return "Done";
  }

  if ("WorkspaceChanges" in command) {
    const found = place(command.WorkspaceChanges.workspace);
    if (found.kind.kind === "container")
      refuse("bad_request", "a run's container keeps its own clone; its changes are the run's");
    if (!found.kind.branch)
      refuse(
        "bad_request",
        "that is a plain folder, not a git repository: git cannot tell what changed there",
      );
    const changes = flows.changes[found.id] ?? {
      base: "origin/main",
      uncommitted: [],
      commits: [],
      more_commits: 0,
      branch: [],
    };
    return { Changes: changes };
  }

  if ("TidyCommits" in command) {
    const { workspace, harness } = command.TidyCommits;
    if (!takesPrompt(harness) || harness === "review")
      refuse("bad_request", `"${harness}" is not a coding agent that can tidy commits`);
    const found = place(workspace);
    if (found.kind.kind === "container")
      refuse("bad_request", "commits are tidied in a worktree or the project's checkout");
    if (!found.kind.branch)
      refuse(
        "bad_request",
        "that is a plain folder, not a git repository: it has no branch to review or push",
      );
    const busy = atWork(workspace);
    if (busy)
      refuse("conflict", `${busy.title} is at work there: tidy the commits once it has stopped`);
    const changes = flows.changes[workspace];
    if (changes?.uncommitted.length)
      refuse(
        "bad_request",
        "some changes are not committed: commit them or put them aside first, as a tidy moves only commits",
      );
    if (!changes?.commits.length)
      refuse("bad_request", `there are no commits since ${changes?.base ?? "origin/main"} to tidy`);
    const started: Session = {
      id: id(),
      workspace,
      harness,
      title: `Tidy commits · ${label(harness)}`,
      status: { state: "running" },
      unseen: false,
      resume_id: null,
      created_at: NOW,
      ended_at: null,
      status_since: NOW,
      activity: null,
      spend: null,
      tidy: { commits: changes.commits.length, from: "a1b2c3d", checked: null },
      beside: null,
    };
    world.sessions.push(started);
    tell({ sessions: [started] });
    return { SessionStarted: { id: started.id } };
  }

  if ("MoveOnto" in command) {
    const found = place(command.MoveOnto.workspace);
    if (found.kind.kind === "container")
      refuse("bad_request", "a run's container keeps its own clone; its changes are the run's");
    if (!found.kind.branch)
      refuse(
        "bad_request",
        "that is a plain folder, not a git repository: git cannot tell what changed there",
      );
    const busy = atWork(found.id);
    if (busy)
      refuse("conflict", `${busy.title} is at work there: move the branch once it has stopped`);
    const changes = flows.changes[found.id];
    if (changes?.uncommitted.length)
      refuse(
        "conflict",
        "the branch did not move: some changes are not committed: commit them or put them aside first",
      );
    const stacked = found.stacked_on;
    const parent = stacked
      ? world.workspaces.find(
          (w) => w.project === found.project && w.kind.branch === stacked.branch,
        )
      : undefined;
    const onParent = stacked && parent && parent.pull_request?.state !== "merged";
    const onto = onParent ? stacked.branch : (changes?.base ?? "origin/main");
    const entry = (found.timeline ?? 0) + 1;
    const moved: Workspace = { ...found, timeline: entry, stacked_on: onParent ? stacked : null };
    world.workspaces = world.workspaces.map((w) => (w.id === found.id ? moved : w));
    tell({ workspaces: [moved] });
    return { Moved: { onto, commits: changes?.commits.length ?? 0, entry } };
  }

  if ("ListIssues" in command) {
    const found = project(command.ListIssues.project);
    return {
      Issues: {
        issues: flows.issues[found.id] ?? [],
        tracker: "GitHub",
        mine: true,
        source: null,
      },
    };
  }
  return undefined;
}
