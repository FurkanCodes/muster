// Which lines of the tree are folded: the window's own, kept per project
// (state/kept.ts) so they are folded again after a restart, a ⌘Q or a
// crash. The daemon is never told, as the terminal UI never tells it.

import type { DaemonState } from "../../state/daemon";
import { keep, kept } from "../../state/kept";

const KEY = "muster.sessions.folds";

/** What is kept: for each project, the lines of it folded (its own id among them). */
interface Kept {
  version: 1;
  projects: Record<string, string[]>;
}

/** The folds kept, or none where nothing or something else is kept. */
export function readFolds(): Set<string> {
  try {
    const folds = JSON.parse(kept(KEY) ?? "null") as Kept | null;
    if (folds?.version !== 1 || typeof folds.projects !== "object") return new Set();
    return new Set(Object.values(folds.projects).flat());
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
  keep(KEY, JSON.stringify({ version: 1, projects } satisfies Kept));
}
