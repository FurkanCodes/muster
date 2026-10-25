// A project's menu (`PRODUCT.md` §6a, `ARC-33`), under the ⋯ on its page
// and at a right-click on its line: each other space it can move to, and a
// new space made for it. It is in one space always; a space only groups.

import { useShallow } from "zustand/react/shallow";
import { useApp, useDaemon } from "../app/context";
import type { ProjectId } from "../ipc/generated/ProjectId";
import type { MenuItem } from "../ui/Menu";
import { moveItems } from "./moveItems";

export function useProjectMenu(project: ProjectId): MenuItem[] {
  const app = useApp();
  const spaces = useDaemon(useShallow((state) => state.spaces));
  return moveItems(app, spaces, { Project: project });
}
