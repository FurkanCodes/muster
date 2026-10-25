// The menu items that move a project or a terminal in no project to
// another space (`ARC-33`): each space it is not in, and a new one.

import type { App } from "../app/context";
import type { Member } from "../ipc/generated/Member";
import type { Space } from "../ipc/generated/Space";
import type { MenuItem } from "../ui/Menu";
import { moveTo } from "./spaces";

export function moveItems(app: App, spaces: readonly Space[], member: Member): MenuItem[] {
  const key = JSON.stringify(member);
  return [
    ...spaces
      .filter((space) => !space.members.some((held) => JSON.stringify(held) === key))
      .map((space) => ({
        key: "",
        label: `Move to ${space.name}`,
        does: () => void moveTo(app, space.id, member),
      })),
    {
      key: "",
      label: "Move to a new space…",
      does: () =>
        app.actions.openFlow({ kind: "space-name", space: null, after: { kind: "move", member } }),
    },
  ];
}
