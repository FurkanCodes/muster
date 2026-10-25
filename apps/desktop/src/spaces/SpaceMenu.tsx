// The projects' heading as the way among spaces (`PRODUCT.md` §6a): with
// no space yet it reads Projects, as it always did; once there is one, it
// names the space the window shows and opens a menu of them, of making,
// renaming and removing one. The window's own choice: the daemon is never
// told which space a window shows.

import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon, useUi } from "../app/context";
import { useKeymap } from "../keys/useKeymap";
import { Pressable } from "../ui/controls";
import { Icon } from "../ui/Icon";
import { ActionMenu, keysOf, type MenuItem } from "../ui/Menu";
import { goTo, spaceShown } from "./spaces";
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
