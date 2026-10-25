// What the mock daemon derives from its state: the same rules the Rust side
// takes from the shared model (`muster-model`), kept to what tests need.
// The real app never runs this: its derivations come from Rust.

import type { Derived } from "../generated/desktop/Derived";
import type { Row } from "../generated/desktop/Row";
import type { Member } from "../generated/Member";
import type { Project } from "../generated/Project";
import type { Session } from "../generated/Session";
import type { SessionId } from "../generated/SessionId";
import type { Space } from "../generated/Space";
import type { Workspace } from "../generated/Workspace";

/** A shell, or a plugin's panel: the person's own or a plugin's, never an agent's chat. */
export function standsBeside(harness: string): boolean {
  return harness === "shell" || harness.startsWith("plugin:");
}

/** Whether a session waits on the person (`muster_core::news::needs_attention`). */
export function needsAttention(session: Session): boolean {
  return !standsBeside(session.harness) && (session.status.state === "needs_you" || session.unseen);
}

export interface World {
  projects: Project[];
  workspaces: Workspace[];
  sessions: Session[];
  /** What each session's terminal shows, as its program wrote it. */
  terminals?: Readonly<Record<string, string>>;
  /** The person's settings, of which the projects' order lists them. */
  preferences?: { projects: readonly string[] };
  /** The person's spaces. */
  spaces?: Space[];
  /** The folder each terminal's shell is in, where it is not the home folder. */
  folders?: Record<string, string>;
}

/** The lines a question shows, at most (`link/questions.rs`, `LINES`). */
const QUESTION_LINES = 4;

// Escape sequences: CSI (colours, moves) and OSC (titles), as a screen
// takes them rather than shows them.
// biome-ignore lint/suspicious/noControlCharactersInRegex: escape sequences are what it matches.
const ESCAPES = /\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g;
// biome-ignore lint/suspicious/noControlCharactersInRegex: control characters are what it takes out.
const CONTROLS = /[\x00-\x1f\x7f]/g;

/**
 * The last lines with anything on them of what a program wrote, as the Rust
 * side reads its screen for a question (`link/questions.rs`): escape
 * sequences and control characters out, trailing blanks off, a line
 * rewritten after a carriage return read as rewritten.
 */
export function lastLines(text: string, count = QUESTION_LINES): string[] {
  return text
    .replace(ESCAPES, "")
    .split(/\r?\n/)
    .map((line) =>
      line
        .split("\r")
        .reduce((shown, over) => over + shown.slice(over.length), "")
        .replace(CONTROLS, "")
        .trimEnd(),
    )
    .filter((line) => line.trim() !== "")
    .slice(-count);
}

/** Each asking session's question, while the window is seen (`Derived.questions`). */
function questionsOf(world: World): Derived["questions"] {
  const questions: Derived["questions"] = {};
  for (const session of world.sessions) {
    if (session.status.state !== "needs_you") continue;
    const lines = lastLines(world.terminals?.[session.id] ?? "");
    if (lines.length > 0) questions[session.id] = lines;
  }
  return questions;
}

/** A project's workspaces: its checkout first, then worktrees oldest first, then containers. */
export function workspacesOf(world: World, project: string): Workspace[] {
  const rank = (workspace: Workspace) =>
    workspace.kind.kind === "checkout" ? 0 : workspace.kind.kind === "worktree" ? 1 : 2;
  return world.workspaces
    .filter((workspace) => workspace.project === project)
    .sort((a, b) => rank(a) - rank(b) || a.created_at - b.created_at);
}

/** A workspace's sessions as listed: its chats, then its shells and panels. */
export function listedOf(world: World, workspace: string): Session[] {
  const mine = world.sessions.filter((session) => session.workspace === workspace);
  return [
    ...mine.filter((session) => !standsBeside(session.harness)),
    ...mine.filter((session) => standsBeside(session.harness)),
  ];
}

export function derive(
  world: World,
  changed: ReadonlyMap<SessionId, number>,
  seen = true,
): Derived {
  const asking = (session: Session) => session.status.state === "needs_you";
  const needsYou = world.sessions
    .filter(needsAttention)
    .sort(
      (a, b) =>
        Number(!asking(a)) - Number(!asking(b)) ||
        (changed.get(a.id) ?? 0) - (changed.get(b.id) ?? 0),
    )
    .map((session) => session.id);

  const rows: Row[] = [];
  // In the person's order, the others after them as they were added
  // (`Model::order_projects`).
  const order = world.preferences?.projects ?? [];
  const at = (id: string) => {
    const found = order.indexOf(id);
    return found < 0 ? order.length : found;
  };
  const projects = [...world.projects].sort((one, other) => at(one.id) - at(other.id));
  // `Model::rows_in`: a space's projects, then its terminals in no project.
  const listed = (shown: (member: Member) => boolean): Row[] => {
    const out: Row[] = [];
    for (const project of projects) {
      if (!shown({ Project: project.id })) continue;
      out.push({ kind: "project", id: project.id });
      for (const workspace of workspacesOf(world, project.id)) {
        out.push({ kind: "workspace", id: workspace.id });
        for (const session of listedOf(world, workspace.id)) {
          out.push({ kind: "session", id: session.id });
        }
      }
    }
    const free = world.sessions.filter(
      (session) => session.workspace === null && shown({ Session: session.id }),
    );
    if (free.length > 0) {
      out.push({ kind: "terminals" });
      for (const session of free) out.push({ kind: "session", id: session.id });
    }
    return out;
  };
  rows.push(...listed(() => true));
  // `Model::in_spaces`, for each project and each session waiting.
  const noteOf = (member: Member) => {
    const names = (world.spaces ?? [])
      .filter((space) =>
        space.members.some((held) => JSON.stringify(held) === JSON.stringify(member)),
      )
      .map((space) => space.name);
    return names.length ? `in ${names.join(", ")}` : "";
  };
  const space_notes: Record<string, string> = {};
  for (const project of world.projects) {
    const note = noteOf({ Project: project.id });
    if (note) space_notes[project.id] = note;
  }
  for (const id of needsYou) {
    const session = world.sessions.find((one) => one.id === id);
    const workspace = world.workspaces.find((one) => one.id === session?.workspace);
    const note = session
      ? noteOf(workspace ? { Project: workspace.project } : { Session: session.id })
      : "";
    if (note) space_notes[id] = note;
  }
  const space_rows: Record<string, Row[]> = {};
  for (const space of world.spaces ?? [])
    space_rows[space.id] = listed((member) =>
      space.members.some((held) => JSON.stringify(held) === JSON.stringify(member)),
    );

  const chats = world.sessions.filter((session) => !standsBeside(session.harness));
  // The agents at work, the longest at it first (`Model::working`).
  const working = chats
    .filter((session) => session.status.state === "running" && !needsAttention(session))
    .sort((a, b) => a.status_since - b.status_since)
    .map((session) => session.id);
  return {
    needs_you: needsYou,
    working,
    rows,
    space_rows,
    space_notes,
    counts: {
      working: chats.filter((session) => session.status.state === "running").length,
      waiting: chats.filter(asking).length,
      done: chats.filter((session) => session.status.state === "finished").length,
    },
    questions: seen ? questionsOf(world) : {},
  };
}
