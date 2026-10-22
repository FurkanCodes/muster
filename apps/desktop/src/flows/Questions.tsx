// The questions a flow asks about a session, a worktree or a project,
// loaded with the first one asked.

import { Move, Tidy } from "./BranchDialogs";
import { RemoveProject, RemoveWorktree, Stop } from "./SessionDialogs";
import type { Flow } from "./types";

export type Question = Exclude<Flow, { kind: "add-project" | "new-worktree" }>;

export default function Questions({ flow }: { flow: Question }) {
  switch (flow.kind) {
    case "stop":
      return <Stop session={flow.session} />;
    case "remove-worktree":
      return <RemoveWorktree workspace={flow.workspace} />;
    case "remove-project":
      return <RemoveProject project={flow.project} />;
    case "tidy":
      return <Tidy workspace={flow.workspace} />;
    case "move":
      return <Move workspace={flow.workspace} />;
  }
}
