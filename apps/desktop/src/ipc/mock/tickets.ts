// The tickets of the mock daemon's working day, and its answers about them,
// as supervisor/issues.rs, settings.rs and worktrees.rs give them: the
// Tickets board's (design/screens/Tickets.html).

import { BridgeFailure } from "../bridge";
import type { Command } from "../generated/Command";
import type { IssueComment } from "../generated/IssueComment";
import type { Project } from "../generated/Project";
import type { ProjectSettings } from "../generated/ProjectSettings";
import type { Reply } from "../generated/Reply";
import type { Session } from "../generated/Session";
import type { Workspace } from "../generated/Workspace";
import type { MockBridge } from "./bridge";
import { NOW } from "./now";

type Issues = Extract<Reply, { Issues: unknown }>["Issues"];
type Viewed = Extract<Reply, { IssueViewed: unknown }>["IssueViewed"];
export interface TicketScenario {
  /** Each project's open tickets, as `ListIssues` answers. */
  tickets: Record<string, Issues>;
  /** Each ticket's text, by `<project>:<key>`, as `ViewIssue` answers. */
  ticketTexts: Record<string, Viewed>;
  ticketSettings: Record<string, ProjectSettings>;
}

/** What was said on #214 after it was filed, oldest first. */
const BOARD_CONVERSATION: IssueComment[] = [
  {
    author: "mira-k",
    at: NOW - 26 * 60 * 60 * 1000,
    body: "Does the window keep its **own** connection, or share the TUI's? If it shares it, closing the TUI would take the window down with it.",
  },
  {
    author: "sam",
    at: NOW - 5 * 60 * 60 * 1000,
    body: "Its own: `muster-client` opens one per client.\n\n- the daemon already serves many\n- nothing changes on its side\n\nSee [ARCHITECTURE §5c](https://github.com/o/muster/blob/main/ARCHITECTURE.md).",
  },
  {
    author: null,
    at: null,
    body: "> nothing changes on its side\n\nGood, then this can ship on its own.",
  },
];

const BOARD_TEXT = `The first slice of Muster Desktop: a window that lists sessions and shows one live terminal, as another client of the daemon. Nothing about the daemon changes.

### Done when
- The window connects through \`muster-client\` and shows the Needs you queue.
- Selecting a session streams its terminal into the pane beside the queue, and typing reaches its program.
- Closing the window stops nothing, and opening it again picks up where it was.

### Not yet
Tickets, Settings and the changes pane come after. The terminal UI stays as it is.`;

const YOUTRACK_TEXT = `A ticket plugin for YouTrack, like the Jira one: it lists a project's open issues as tickets, keyed as YouTrack keys them.

It asks for the site and, if you like, a query. The token lives in the Keychain.`;

/** muster, lamphaus, then kep-site and kep-api sharing one Linear team. */
const TICKETS: [key: string, title: string, labels: string[]][][] = [
  [
    ["214", "Desktop app: sessions list and one live terminal", ["desktop", "enhancement"]],
    ["205", "Resize settles once on release", ["bug"]],
    ["198", "Light mode for the notch", ["notch"]],
    ["217", "Tickets from YouTrack", ["plugins"]],
    ["220", "Fold the list from the notch", []],
  ],
  [
    ["212", "Share the Gradle cache across worktrees", ["build"]],
    ["219", "Upgrade AGP to 9.1", []],
    ["224", "Crash when rotating on the scanner screen", ["bug"]],
  ],
  [
    ["KEP-31", "Sign-up page loads the old pricing", []],
    ["KEP-28", "Move the docs to the new domain", []],
  ],
];

export function ticketScenario(projects: Project[]): TicketScenario {
  const scenario: TicketScenario = { tickets: {}, ticketTexts: {}, ticketSettings: {} };
  projects.forEach((project, at) => {
    const linear = at >= 2;
    const issues = (TICKETS[Math.min(at, 2)] ?? []).map(([key, title, labels]) => ({
      key,
      title,
      url: linear
        ? `https://linear.app/kep/issue/${key}`
        : `https://github.com/FurkanCodes/${project.name}/issues/${key}`,
      labels,
      author: "FurkanCodes",
    }));
    const mine = project.name === "lamphaus";
    scenario.tickets[project.id] = {
      issues,
      tracker: linear ? "Linear (KEP)" : "GitHub issues",
      mine,
      // A tracker that is no repository's own says where it is; GitHub's says nothing.
      source: linear ? "linear KEP all" : null,
    };
    scenario.ticketSettings[project.id] = {
      tickets_from: linear ? "Linear (KEP)" : "GitHub issues",
      only_mine: { on: mine, from: mine ? "yours" : "default" },
      file: `~/.config/muster/projects/${project.id}.toml`,
      tickets: null,
      tickets_set_by: "default",
      tickets_untold: false,
      muster_prefix: { on: true, from: "default" },
    };
    for (const issue of issues)
      scenario.ticketTexts[`${project.id}:${issue.key}`] = {
        issue,
        body: issue.key === "217" ? YOUTRACK_TEXT : issue.key === "214" ? BOARD_TEXT : "",
        comments: issue.key === "214" ? BOARD_CONVERSATION : [],
      };
  });
  return scenario;
}

/**
 * Puts the board's agents on their tickets: muster's two worktrees made
 * for #214 and #205, on branches named for them, and a kep-api worktree for KEP-31 with
 * its Gemini in it. The working day keeps its own branch names for the other
 * screens; a test that wants the board calls this.
 */
export function agentsOnTickets(mock: MockBridge): void {
  const world = mock.world;
  const changed: Workspace[] = [];
  const rename = (workspace: Workspace | undefined, branch: string, ticket: string) => {
    if (workspace?.kind.kind !== "worktree") return;
    workspace.kind = { ...workspace.kind, branch };
    workspace.ticket = ticket;
    changed.push({ ...workspace });
  };
  const of = (project: string) => world.workspaces.filter((w) => w.project === project);
  const [muster, , , kepApi] = world.projects;
  const trees = of(muster?.id ?? "").filter((w) => w.kind.kind === "worktree");
  rename(trees[0], "muster/issue-214-desktop-app-sessions-list-and-one-live", "214");
  rename(trees[1], "muster/issue-205-resize-settles-once-on-release", "205");
  const checkout = of(kepApi?.id ?? "").find((w) => w.kind.kind === "checkout");
  const gemini = world.sessions.find((s) => s.workspace === checkout?.id);
  if (kepApi && gemini) {
    const tree = worktree(
      kepApi.id,
      "muster/issue-KEP-31-sign-up-page-loads-the-old-pricing",
      1,
      "KEP-31",
    );
    world.workspaces.push(tree);
    changed.push(tree);
    gemini.workspace = tree.id;
    mock.publish({ workspaces: changed, sessions: [{ ...gemini }] });
    return;
  }
  mock.publish({ workspaces: changed });
}

function worktree(project: string, branch: string, n: number, ticket: string | null): Workspace {
  return {
    id: `01JT${String(n).padStart(22, "0")}`,
    project,
    kind: { kind: "worktree", path: `/tmp/mock/${branch}`, branch },
    created_at: NOW,
    pull_request: null,
    changes: null,
    timeline: null,
    clashes: [],
    stacked_on: null,
    ticket,
  };
}

/** `ticket_branch` in muster-core: the key, then whole words of the title up to 40. */
function ticketBranch(key: string, title: string, prefixed: boolean): string {
  let slug = "";
  for (const word of title
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean)) {
    if (slug.length + word.length + 1 > 40) break;
    slug += `${slug ? "-" : ""}${word}`;
  }
  return `${prefixed ? "muster/" : ""}issue-${key}${slug ? `-${slug}` : ""}`;
}

function refused(message: string, code: "not_found" | "bad_request" | "conflict"): BridgeFailure {
  return new BridgeFailure("refused", message, code);
}

export function ticketCommand(mock: MockBridge, command: Command): Reply | undefined {
  if (typeof command === "string") return undefined;
  const world = mock.world;
  if ("ProjectSettings" in command) {
    const { project } = command.ProjectSettings;
    const setting = world.ticketSettings[project];
    if (!setting) throw refused(`no project prj_${project}`, "not_found");
    return { ProjectSettings: structuredClone(setting) };
  }
  if ("ListIssues" in command) {
    const { project } = command.ListIssues;
    const list = world.tickets[project];
    if (!list) throw refused(`no project prj_${project}`, "not_found");
    return { Issues: structuredClone(list) };
  }
  if ("ViewIssue" in command) {
    const { project, key } = command.ViewIssue;
    const detail = world.ticketTexts[`${project}:${key}`];
    if (!detail) throw refused(`no ticket ${key}`, "not_found");
    return { IssueViewed: structuredClone(detail) };
  }
  if ("ChangeSetting" in command) {
    const { project, change } = command.ChangeSetting;
    const setting = world.ticketSettings[project];
    if (!setting) throw refused(`no project prj_${project}`, "not_found");
    if (!("OnlyMyTickets" in change)) return undefined;
    setting.only_mine = { on: change.OnlyMyTickets, from: "yours" };
    const list = world.tickets[project];
    if (list) {
      list.mine = change.OnlyMyTickets;
      // Linear's source says whose tickets it lists, so only alike projects share.
      if (list.source)
        list.source = list.source.replace(/ (mine|all)$/, list.mine ? " mine" : " all");
    }
    return { ProjectSettings: structuredClone(setting) };
  }
  if ("CloseIssue" in command) {
    const { project, key } = command.CloseIssue;
    const list = world.tickets[project];
    if (!list) throw refused(`no project prj_${project}`, "not_found");
    for (const [id, other] of Object.entries(world.tickets))
      if (id === project || (list.source !== null && other.source === list.source))
        other.issues = other.issues.filter((i) => i.key !== key);
    return "Done";
  }
  if ("RemoveWorktree" in command) return removeWorktree(mock, command.RemoveWorktree);
  if ("StartFromIssue" in command) return startFromIssue(mock, command.StartFromIssue);
  return undefined;
}

function removeWorktree(
  mock: MockBridge,
  { workspace, force }: { workspace: string; force: boolean },
): Reply {
  const world = mock.world;
  const found = world.workspaces.find((w) => w.id === workspace);
  if (!found) throw refused(`no worktree wsp_${workspace}`, "not_found");
  if (found.kind.kind !== "worktree")
    throw refused(
      "that is the project's own checkout, which Muster never removes; remove the project instead",
      "bad_request",
    );
  const running = world.sessions.filter((s) => s.workspace === workspace && s.ended_at === null);
  if (running.length)
    throw refused(`stop the worktree's ${running.length} running session(s) first`, "conflict");
  if (!force && found.changes?.files)
    throw refused(
      `${found.kind.path} has uncommitted changes; remove it with force to discard them`,
      "conflict",
    );
  const gone = world.sessions.filter((s) => s.workspace === workspace).map((s) => s.id);
  world.sessions = world.sessions.filter((s) => s.workspace !== workspace);
  world.workspaces = world.workspaces.filter((w) => w.id !== workspace);
  mock.publish({ gone: { projects: [], workspaces: [workspace], sessions: gone } });
  return "Done";
}

function startFromIssue(
  mock: MockBridge,
  start: Extract<Command, { StartFromIssue: unknown }>["StartFromIssue"],
): Reply {
  const world = mock.world;
  const { project, key, harness, another, on } = start;
  const detail = world.ticketTexts[`${project}:${key}`];
  if (!detail) throw refused(`no ticket ${key}`, "not_found");
  const own = ["claude", "codex", "gemini", "opencode", "copilot", "shell"];
  if (!own.includes(harness) && !world.preferences.agents.some((a) => a.name === harness))
    throw refused(`no harness ${harness}`, "bad_request");
  const branch = ticketBranch(
    key,
    detail.issue.title,
    world.ticketSettings[project]?.muster_prefix?.on !== false,
  );
  const inProject = (b: string) =>
    world.workspaces.find((w) => w.project === project && w.kind.branch === b);
  // Its worktree, made for it, whatever its branch is called now.
  let workspace = another
    ? undefined
    : world.workspaces.find(
        (w) => w.project === project && w.kind.kind === "worktree" && w.ticket === key,
      );
  if (!workspace) {
    // Another agent's worktree is `…-2`, `…-3`: the first nobody has.
    let n = 2;
    let chosen = another ? `${branch}-${n}` : branch;
    while (another && inProject(chosen)) chosen = `${branch}-${++n}`;
    const base = on ? world.workspaces.find((w) => w.id === on) : undefined;
    if (on && (!base || base.project !== project || base.kind.kind !== "worktree"))
      throw refused("the worktree to build on is not in this project", "bad_request");
    workspace = {
      ...worktree(project, chosen, world.workspaces.length + 100, key),
      stacked_on: base ? { branch: base.kind.branch, at: "0a1b2c3" } : null,
    };
    world.workspaces.push(workspace);
  }
  const session: Session = {
    id: `01JS${String(world.sessions.length + 100).padStart(22, "0")}`,
    workspace: workspace.id,
    harness,
    title: `${/^\d+$/.test(key) ? "#" : ""}${key} ${detail.issue.title}`,
    status: { state: "fresh" },
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
  world.sessions.push(session);
  mock.publish({ workspaces: [{ ...workspace }], sessions: [session] });
  return { IssueStarted: { workspace: workspace.id, session: session.id } };
}
