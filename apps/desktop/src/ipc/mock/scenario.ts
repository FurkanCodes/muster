// A working day, as the design canvas draws it: four projects, agents at
// work, two asking, some finished. The mock daemon starts from it, and tests
// change it through the mock bridge.

import type { Changes } from "../generated/Changes";
import type { FileDiff } from "../generated/FileDiff";
import type { PluginInfo } from "../generated/PluginInfo";
import type { Preferences } from "../generated/Preferences";
import type { Project } from "../generated/Project";
import type { Session } from "../generated/Session";
import type { SessionStatus } from "../generated/SessionStatus";
import type { SessionUsage } from "../generated/SessionUsage";
import type { Timeline } from "../generated/Timeline";
import type { UndoPlan } from "../generated/UndoPlan";
import type { Workspace } from "../generated/Workspace";
import type { World } from "./model";
import { workingDayPlugins } from "./plugins";

/** A ULID-shaped id: the daemon sends ids as 26 Crockford base-32 characters. */
export function ulid(n: number): string {
  return `01JD${String(n).padStart(22, "0")}`;
}

const MINUTE = 60_000;

import { NOW } from "./now";
import { type TicketScenario, ticketScenario } from "./tickets";

export { NOW };

export interface Scenario extends World, TicketScenario {
  preferences: Preferences;
  plugins: PluginInfo[];
  /** What each session's terminal shows when attached. */
  terminals: Record<string, string>;
  /** Each workspace's timeline, as `Timeline` answers it. */
  timelines?: Record<string, Timeline>;
  /** Each timeline entry's diff, as `EntryDiff` answers it. */
  entryDiffs?: Record<string, Record<number, FileDiff>>;
  /** What sessions use, as the daemon's measurement says. */
  usage?: SessionUsage[];
  /** Each workspace's changes, as `WorkspaceChanges` answers them. */
  changes?: Record<string, Changes>;
  /** Each file's diff, as `FileDiff` answers it, by `workspace:part:path`. */
  diffs?: Record<string, FileDiff>;
  /** What putting the files back before an entry would do, by `workspace:number`. */
  undoPlans?: Record<string, UndoPlan>;
  /** The listing as it was before each entry, for `Undo` to put back, by `workspace:number`. */
  beforeUndo?: Record<string, Changes>;
}

function project(n: number, name: string, root: string): Project {
  return { id: ulid(n), name, root, created_at: NOW - 30 * 24 * 60 * MINUTE };
}

function workspace(
  n: number,
  projectN: number,
  kind: "checkout" | "worktree",
  branch: string,
  path: string,
  extra: Partial<Workspace> = {},
): Workspace {
  return {
    id: ulid(n),
    project: ulid(projectN),
    kind: { kind, path, branch },
    created_at: NOW - n * MINUTE,
    pull_request: null,
    changes: null,
    timeline: null,
    clashes: [],
    stacked_on: null,
    // Made for the ticket its name says, as the daemon names one's.
    ticket: /^muster\/issue-(\d+)-/.exec(branch)?.[1] ?? null,
    ...extra,
  };
}

function session(
  n: number,
  workspaceN: number,
  harness: string,
  title: string,
  status: SessionStatus,
  minutesAgo: number,
  extra: Partial<Session> = {},
): Session {
  return {
    id: ulid(n),
    workspace: ulid(workspaceN),
    harness,
    title,
    status,
    unseen: false,
    resume_id: null,
    created_at: NOW - (minutesAgo + 30) * MINUTE,
    ended_at: null,
    status_since: NOW - minutesAgo * MINUTE,
    activity: null,
    spend: null,
    tidy: null,
    beside: null,
    started_by: null,
    agent: null,
    ...extra,
  };
}

const activity = (text: string, minutesAgo: number) => ({
  text,
  since: NOW - minutesAgo * MINUTE,
});

export function defaultPreferences(): Preferences {
  return {
    notch: true,
    file: "~/.config/muster/settings.toml",
    agents: [],
    problem: null,
    side_by_side_from: 100,
    palette: "muster",
    session_lines: "two",
    panels: "list | session 60% | (changes / shell / plugin)",
    list_folded: false,
    naming: { by_agent: true, agent: null, model: null },
    keys: {},
    notifications: true,
    telling: {
      questions: { on: true, sound: "Glass" },
      finishes: { on: true, sound: "default" },
      errors: { on: true, sound: "Basso" },
    },
    shared_builds: true,
    delegation: { allowed: true, at_once: 4, depth: 2 },
  };
}

export function workingDay(): Scenario {
  const projects = [
    project(1, "muster", "/Users/you/muster"),
    project(2, "lamphaus", "/Users/you/code/lamphaus"),
    project(3, "kep-site", "/Users/you/code/kep-site"),
    project(4, "kep-api", "/Users/you/code/kep-api"),
  ];
  const workspaces = [
    workspace(10, 1, "checkout", "main", "/Users/you/muster", {
      changes: { files: 2, added: 12, removed: 3 },
    }),
    workspace(11, 1, "worktree", "feature/desktop-app", "~/.muster/worktrees/muster/desktop", {
      // As its listing below sums: the uncommitted files and the branch's.
      changes: { files: 3, added: 300, removed: 6 },
      pull_request: {
        number: 91,
        url: "https://github.com/FurkanCodes/muster/pull/91",
        state: "draft",
        checks: "pending",
        review: null,
      },
      timeline: 4,
    }),
    workspace(12, 1, "worktree", "fix/resize-under-load", "~/.muster/worktrees/muster/resize", {
      changes: { files: 4, added: 64, removed: 22 },
      pull_request: {
        number: 88,
        url: "https://github.com/FurkanCodes/muster/pull/88",
        state: "open",
        checks: "passing",
        review: "approved",
      },
      clashes: [
        {
          with: ulid(11),
          files: 1,
          conflicts: 0,
          shown: [{ path: "crates/muster-client/src/connection.rs", conflict: false }],
        },
      ],
    }),
    workspace(20, 2, "checkout", "main", "/Users/you/code/lamphaus"),
    workspace(
      21,
      2,
      "worktree",
      "muster/issue-212-gradle-cache",
      "~/.muster/worktrees/lamphaus/212",
    ),
    workspace(22, 2, "worktree", "muster/issue-219-agp-9-1", "~/.muster/worktrees/lamphaus/219", {
      changes: { files: 6, added: 41, removed: 38 },
    }),
    workspace(30, 3, "checkout", "main", "/Users/you/code/kep-site"),
    workspace(40, 4, "checkout", "main", "/Users/you/code/kep-api"),
  ];
  const sessions = [
    session(100, 10, "claude", "Split view.rs by responsibility", { state: "running" }, 4, {
      activity: activity("editing view/sessions.rs", 1),
      spend: { tokens: 1_200_000, cost_micros: null },
    }),
    session(
      101,
      11,
      "codex",
      "Sketch the Tauri shell",
      { state: "needs_you", reason: "permission" },
      1,
      {
        activity: activity("wants to run cargo build", 1),
        spend: { tokens: 182_000, cost_micros: null },
      },
    ),
    session(102, 11, "shell", "git log", { state: "running" }, 0),
    session(103, 12, "claude", "Flaky resize test under load", { state: "running" }, 2, {
      activity: activity("running cargo nextest", 2),
    }),
    session(104, 12, "claude", "Resize settles once on release", { state: "finished" }, 12, {
      unseen: true,
    }),
    session(
      200,
      21,
      "gemini",
      "Share the Gradle cache across worktrees",
      { state: "needs_you", reason: "question" },
      3,
      {
        activity: activity("asks which cache folder to use", 3),
      },
    ),
    session(201, 22, "opencode", "Upgrade AGP to 9.1", { state: "running" }, 8, {
      activity: activity("running ./gradlew assembleDebug", 8),
    }),
    session(202, 20, "copilot", "Explain the release script", { state: "finished" }, 60, {
      unseen: true,
    }),
    session(400, 40, "gemini", "Sign-up page loads the old pricing", { state: "running" }, 1, {
      activity: activity("reading app/pricing.ts", 1),
    }),
  ];
  const terminals: Record<string, string> = {
    [ulid(101)]: [
      "\x1b[32m•\x1b[0m Edited crates/muster-desktop/src/link.rs \x1b[32m(+118\x1b[0m \x1b[31m-4)\x1b[0m",
      "",
      "  The link subscribes once, mirrors the snapshot and its deltas,",
      "  and hands each change to the webview on a Tauri channel.",
      "",
      "  \x1b[33mAllow Codex to run this command?\x1b[0m",
      "",
      "  $ cargo build -p muster-desktop",
      "",
      "  \x1b[35m› 1. Yes, run it\x1b[0m",
      "    2. No, and tell Codex what to do differently",
    ].join("\r\n"),
    [ulid(100)]:
      "\x1b[32m●\x1b[0m Update(crates/muster-tui/src/view/sessions.rs)\r\n  ⎿  Moved 212 lines to view/rows.rs\r\n\r\n\x1b[33m✻\x1b[0m Splitting sessions.rs…\r\n",
    [ulid(102)]:
      "\x1b[35m❯\x1b[0m git log --oneline -3\r\na1b2c3d Add the desktop crate\r\n\x1b[35m❯\x1b[0m ",
    // Gemini CLI clears its screen to ask.
    [ulid(200)]: [
      "  Where should the shared cache live?",
      "  \x1b[36m›\x1b[0m ~/.gradle/caches",
      "    ~/code/lamphaus/.gradle-shared",
    ].join("\r\n"),
  };
  const plugins: PluginInfo[] = workingDayPlugins();
  // The desktop worktree's timeline, newest first: Codex's third turn there
  // is the newest, after the person's own edits between its turns.
  const turn = (number: number, minutesAgo: number, files: number, added: number) => ({
    number,
    kind: { kind: "turn" as const, session: ulid(101), agent: "Codex" },
    started: NOW - (minutesAgo + 3) * MINUTE,
    ended: NOW - minutesAgo * MINUTE,
    summary: { files, added, removed: 4 },
    head_before: null,
    head_after: null,
  });
  const timelines: Record<string, Timeline> = {
    [ulid(11)]: {
      entries: [
        turn(4, 1, 3, 203),
        {
          number: 3,
          kind: { kind: "between" },
          started: NOW - 20 * MINUTE,
          ended: NOW - 5 * MINUTE,
          summary: { files: 1, added: 2, removed: 0 },
          head_before: null,
          head_after: null,
        },
        turn(2, 25, 2, 40),
        turn(1, 40, 1, 12),
      ],
    },
  };
  const diff = (path: string, added: number, removed: number) =>
    [
      `diff --git a/${path} b/${path}`,
      `--- a/${path}`,
      `+++ b/${path}`,
      `@@ -1,${removed} +1,${added} @@`,
      ...Array.from({ length: removed }, () => "-old"),
      ...Array.from({ length: added }, () => "+new"),
      "",
    ].join("\n");
  const entryDiffs: Record<string, Record<number, FileDiff>> = {
    [ulid(11)]: {
      4: {
        text: [
          diff("crates/muster-desktop/src/link.rs", 118, 4),
          diff("crates/muster-desktop/src/main.rs", 61, 0),
          diff("crates/muster-desktop/Cargo.toml", 24, 0),
        ].join(""),
        cut: false,
      },
    },
  };
  // The desktop worktree's changes, as the Changes board shows them.
  const changes: Record<string, Changes> = {
    [ulid(11)]: {
      base: "main",
      more_commits: 0,
      commits: [{ id: "a1b2c3d", subject: "Add the desktop crate" }],
      uncommitted: [
        { path: "src/link.rs", from: null, kind: "modified", lines: { added: 118, removed: 4 } },
        { path: "src/main.rs", from: null, kind: "untracked", lines: { added: 61, removed: 0 } },
        { path: "Cargo.toml", from: null, kind: "untracked", lines: { added: 24, removed: 0 } },
      ],
      branch: [
        { path: "src/link.rs", from: null, kind: "added", lines: { added: 85, removed: 0 } },
        { path: "Cargo.toml", from: null, kind: "modified", lines: { added: 12, removed: 2 } },
      ],
    },
  };
  const link: FileDiff = {
    text: [
      "diff --git a/src/link.rs b/src/link.rs",
      "--- a/src/link.rs",
      "+++ b/src/link.rs",
      "@@ -1,2 +1,3 @@",
      " //! The desktop app's link to the daemon.",
      "-use muster_client::Connection;",
      "+use muster_client::{Connection, Received};",
      "+use tauri::ipc::Channel;",
      "@@ -18,1 +19,1 @@",
      "-// TODO: deltas",
      "+// Hand each change to the webview.",
    ].join("\n"),
    cut: false,
  };
  const diffs: Record<string, FileDiff> = {};
  for (const part of ["uncommitted", "branch"] as const)
    for (const file of changes[ulid(11)]?.[part] ?? [])
      diffs[`${ulid(11)}:${part}:${file.path}`] = link;
  const desktopDiffs = entryDiffs[ulid(11)];
  if (desktopDiffs)
    for (const entry of timelines[ulid(11)]?.entries ?? []) desktopDiffs[entry.number] ??= link;
  const desktop = workspaces.find((found) => found.id === ulid(11));
  if (desktop)
    desktop.clashes = [
      {
        with: ulid(12),
        files: 1,
        conflicts: 0,
        shown: [{ path: "crates/muster-client/src/connection.rs", conflict: false }],
      },
    ];
  // Putting the files back before each entry: the branch has moved one
  // commit since every entry but the newest.
  const undoPlans: Record<string, UndoPlan> = {};
  const beforeUndo: Record<string, Changes> = {};
  const entries = timelines[ulid(11)]?.entries ?? [];
  for (const entry of entries) {
    const key = `${ulid(11)}:${entry.number}`;
    undoPlans[key] = {
      number: entry.number,
      later: entries.filter((other) => other.number > entry.number && other.summary).length,
      branch:
        entry.number < 4
          ? {
              branch: "feature/desktop-app",
              to: "1234567",
              commits: 1,
              forward: false,
              refused: null,
            }
          : null,
    };
    beforeUndo[key] = { base: "main", uncommitted: [], branch: [], commits: [], more_commits: 0 };
  }
  // What the asking Codex session uses, as the daemon measures it.
  const usage: SessionUsage[] = [
    { session: ulid(101), memory_bytes: 212_000_000, cpu_permille: 30 },
  ];
  return {
    projects,
    workspaces,
    sessions,
    preferences: defaultPreferences(),
    plugins,
    terminals,
    ...ticketScenario(projects),
    timelines,
    entryDiffs,
    usage,
    changes,
    diffs,
    undoPlans,
    beforeUndo,
  };
}
