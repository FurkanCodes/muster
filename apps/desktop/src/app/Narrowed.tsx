// What a list narrowed to one project says under its title: which project,
// and the way back to every project, so the filter chosen in the sidebar is
// never a mystery on the screen it filters.

import { projectColour } from "../state/selectors";
import { Icon } from "../ui/Icon";
import { useActions, useDaemon, useUi } from "./context";

export function Narrowed() {
  const actions = useActions();
  const project = useUi((state) => state.project);
  const name = useDaemon((state) => (project ? (state.projects[project]?.name ?? "") : ""));
  const colour = useDaemon((state) => (project ? projectColour(state, project) : 1));
  if (!project) return null;
  return (
    <button
      type="button"
      className="app-narrowed"
      title="Show every project"
      onClick={() => actions.narrow(null)}
    >
      <span className={`mu-swatch mu-p${colour}`} />
      Only {name}
      <Icon name="close" size={12} />
    </button>
  );
}
