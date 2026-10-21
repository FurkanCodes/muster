// Which lines of the tree are folded: the window's own, kept per project in
// this window's storage so they are folded again after a restart. The
// daemon is never told, as the terminal UI never tells it.

import type { DaemonState } from "../../state/daemon";

const KEY = "muster.sessions.folds";

/** What is kept: for each project, the lines of it folded (its own id among them). */
interface Kept {
  version: 1;
  projects: Record<string, string[]>;
}

/** The folds kept, or none where storage is refused or holds something else. */
export function readFolds(): Set<string> {
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? "null") as Kept | null;
    if (kept?.version !== 1 || typeof kept.projects !== "object") return new Set();
    return new Set(Object.values(kept.projects).flat());
  } catch {
    return new Set();
  }
}

/** Keeps `folded`, each line under its project. */
export function writeFolds(state: DaemonState, folded: ReadonlySet<string>): void {
  const projects: Record<string, string[]> = {};
  for (const id of folded) {
    const project = state.projects[id] ? id : state.workspaces[id]?.project;
    if (!project) continue;
    const lines = projects[project] ?? [];
    lines.push(id);
    projects[project] = lines;
  }
  try {
    localStorage.setItem(KEY, JSON.stringify({ version: 1, projects } satisfies Kept));
  } catch {
    // Storage refused (a private window): the folds last as long as the window.
  }
}
