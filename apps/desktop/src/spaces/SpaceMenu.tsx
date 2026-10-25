// The list's heading as the way among spaces (`PRODUCT.md` §6a): with no
// space yet it reads Projects; once there is one, it names the space the
// window shows and opens a menu of them, of making, renaming and removing
// one. The window's own choice: the daemon is never told which space a
// window shows. And a space heading's own menu, at a right-click on it:
// what can go in it, renaming and removing it (`ARC-33`).

import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon, useUi } from "../app/context";
import { addProjectIn } from "../flows/actions";
import { useKeymap } from "../keys/useKeymap";
import { Pressable } from "../ui/controls";
import { Icon } from "../ui/Icon";
import { ActionMenu, keysOf, type MenuItem } from "../ui/Menu";
import { goTo, newTerminal, spaceShown } from "./spaces";
import "./spaces.css";

export function SpaceMenu() {
  const app = useApp();
  const map = useKeymap();
  const chosen = useUi((state) => state.space);
  const open = useUi((state) => state.spacesOpen);
  const spaces = useDaemon(useShallow((state) => state.spaces));
  const shown = useDaemon((state) => spaceShown(state, chosen));
  if (spaces.length === 0) return <span className="app-projects-title">Projects</span>;
  const items: MenuItem[] = [
    { key: shown ? "" : "✓", label: "Everything", does: () => goTo(app, null) },
    ...spaces.map((space) => ({
      key: space.id === shown?.id ? "✓" : "",
      label: space.name,
      does: () => goTo(app, space.id),
    })),
    {
      ...keysOf(map, "spaces"),
      label: "New space…",
      does: () => app.actions.openFlow({ kind: "space-name", space: null }),
    },
    ...(shown
      ? [
          {
            key: "",
            label: `Rename ${shown.name}…`,
            does: () => app.actions.openFlow({ kind: "space-name", space: shown.id }),
          },
          {
            key: "",
            label: `Remove ${shown.name}…`,
            danger: true,
            does: () => app.actions.openFlow({ kind: "remove-space", space: shown.id }),
          },
        ]
      : []),
  ];
  return (
    <ActionMenu
      label="Spaces"
      items={items}
      open={open}
      onOpenChange={(next) => app.actions.openSpaces(next)}
      trigger={
        <Pressable className="app-projects-title spaces-title" aria-label="Spaces">
          {shown ? shown.name : "Everything"}
          <Icon name="chevron" size={12} />
        </Pressable>
      }
    />
  );
}

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
