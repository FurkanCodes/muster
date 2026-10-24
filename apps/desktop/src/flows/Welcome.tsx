// The first run (design/screens/Welcome.html): no projects yet, so Kep and
// the one thing to do, add a project: a folder of the Mac's or a clone from
// GitHub, on their keys, or a folder dropped on the window.

import { useEffect } from "react";
import { useActions, useApp, useDaemon } from "../app/context";
import { Button } from "../ui/controls";
import { EmptyState, EmptyStateActions, EmptyStateText, EmptyStateTitle } from "../ui/EmptyState";
import { KepPortrait } from "../ui/Kep";
import { addProject, cloneProject } from "./actions";
import { labelOf } from "./keymap";
import "./away.css";

export function Welcome() {
  const app = useApp();
  const actions = useActions();
  const key = useDaemon((state) => labelOf(state, "add_project"));
  const cloneKey = useDaemon((state) => labelOf(state, "clone_project"));
  useEffect(() => {
    if (key !== "none") actions.setHints([{ key, does: "add a project" }]);
    return () => actions.setHints([]);
  }, [actions, key]);
  return (
    <EmptyState>
      <KepPortrait />
      <div className="ui-flow-stack">
        <EmptyStateTitle level={1}>Add a project to start</EmptyStateTitle>
        <EmptyStateText>
          A project is a folder, usually a git repository. Muster gathers its agents, worktrees and
          tickets here, and keeps them running when this window closes.
        </EmptyStateText>
      </div>
      <EmptyStateActions>
        <Button
          variant="primary"
          size="lg"
          title={key === "none" ? "Add a project" : `Add a project (${key})`}
          onClick={() => addProject(app)}
        >
          Add a project
        </Button>
        <Button
          variant="secondary"
          size="lg"
          title={cloneKey === "none" ? "Clone from GitHub" : `Clone from GitHub (${cloneKey})`}
          onClick={() => cloneProject(app)}
        >
          Clone from GitHub
        </Button>
      </EmptyStateActions>
      <span className="mu-meta">Or drop a folder on this window.</span>
    </EmptyState>
  );
}
