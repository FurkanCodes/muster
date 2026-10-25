// A project's menu (`PRODUCT.md` §6a, `ARC-33`), under the ⋯ on its page
// and at a right-click on its line: each space it can be put in or taken
// out of, and a new space made with it in. A space only groups: the project
// stays in everything whichever spaces it is in.

import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon } from "../app/context";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { MenuItem } from "../ui/Menu";
import { toggleMember } from "./spaces";

export function useProjectMenu(project: ProjectId): MenuItem[] {
  const app = useApp();
  const spaces = useDaemon(useShallow((state) => state.spaces));
  const member = { Project: project };
  return [
    ...spaces.map((space) => {
      const inIt = space.members.some((held) => "Project" in held && held.Project === project);
      return {
        key: "",
        label: inIt ? `Take out of ${space.name}` : `Put in ${space.name}`,
        does: () => toggleMember(app, space, member, inIt),
      };
    }),
    {
      key: "",
      label: "Put in a new space…",
      does: () => app.actions.openFlow({ kind: "space-name", space: null, join: member }),
    },
  ];
}
