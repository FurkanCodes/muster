// A space heading's own menu, at a right-click on its line (`ARC-33`,
// `PRODUCT.md` §6a): what can go in the space, renaming and removing it.

import { useApp, useDaemon, useUi } from "../app/context";
import { addProjectIn } from "../flows/actions";
import { useKeymap } from "../keys/useKeymap";
import { ActionMenu, keysOf, type MenuItem } from "../ui/Menu";
import { newTerminal } from "./spaces";

/** A space heading's menu, where the right-click on its line asked: what goes in the space, its name, its removal. */
export function SpaceLineMenu() {
  const app = useApp();
  const map = useKeymap();
  const at = useUi((state) => state.menuAt);
  const name = useDaemon((state) =>
    at?.space ? (state.spaces.find((space) => space.id === at.space)?.name ?? null) : null,
  );
  if (!at?.space || name === null) return null;
  const space = at.space;
  // The heading is selected: what is added goes in its space.
  const items: MenuItem[] = [
    { ...keysOf(map, "add_project"), label: "Open a folder…", does: () => addProjectIn(app) },
    {
      ...keysOf(map, "clone_project"),
      label: "Clone from GitHub…",
      does: () => app.actions.openFlow({ kind: "clone-project" }),
    },
    {
      ...keysOf(map, "new_terminal"),
      label: "Terminal",
      does: () => void newTerminal(app),
    },
    {
      key: "",
      label: `Rename ${name}…`,
      does: () => app.actions.openFlow({ kind: "space-name", space }),
    },
    {
      key: "",
      label: `Remove ${name}…`,
      danger: true,
      does: () => app.actions.openFlow({ kind: "remove-space", space }),
    },
  ];
  return (
    <ActionMenu label={name} items={items} at={at} onClose={() => app.actions.openMenuAt(null)} />
  );
}
