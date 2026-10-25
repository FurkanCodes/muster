// The projects in the order the person likes (`PRODUCT.md` §6a, the
// terminal UI's `app::project_order`): a project's line carried up or down
// the list and let go on another project's lines goes there, after it
// moving down and before it moving up, the order kept in the person's own
// settings (`ChangePreference(ProjectOrder)`), so the terminal UI lists
// them so too.

import type { DragEvent } from "react";
import { useApp } from "../../app/context";
import { errorText } from "../../lib/text";
import { CARRIED_PROJECT, carriedProjectId, carryProject } from "../../panels/carry";
import type { DaemonState } from "../../state/daemon";
import type { TreeRow } from "./tree";

/** `order` with `moving` where `onto` is, after it moving down and before it up; null where nothing moves. */
export function movedProjects(
  order: readonly string[],
  moving: string,
  onto: string,
): string[] | null {
  const from = order.indexOf(moving);
  const to = order.indexOf(onto);
  if (from < 0 || to < 0 || from === to) return null;
  const next = order.filter((id) => id !== moving);
  next.splice(next.indexOf(onto) + (from < to ? 1 : 0), 0, moving);
  return next;
}

/** The project whose lines `row` is among. */
function projectOf(state: DaemonState, row: TreeRow): string | null {
  if (row.kind === "project") return row.id;
  const workspace = row.kind === "workspace" ? row.id : (state.sessions[row.id]?.workspace ?? null);
  return workspace ? (state.workspaces[workspace]?.project ?? null) : null;
}

/** The list's drag handlers for a project's line carried over it and let go there. */
export function useProjectDrop(rows: readonly TreeRow[]) {
  const app = useApp();
  const carried = (event: DragEvent) =>
    event.dataTransfer.types.includes(CARRIED_PROJECT) ? carriedProjectId() : null;
  return {
    onDragOver(event: DragEvent<HTMLDivElement>) {
      if (!carried(event)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
    },
    onDrop(event: DragEvent<HTMLDivElement>) {
      const moving = carried(event);
      carryProject(null);
      if (!moving) return;
      event.preventDefault();
      const line = (event.target as HTMLElement | null)?.closest<HTMLElement>("[data-line]");
      const row = line ? rows[Number(line.dataset.line)] : undefined;
      const state = app.daemon.store.getState();
      const onto = row ? projectOf(state, row) : null;
      const order = rows.filter((one) => one.kind === "project").map((one) => one.id);
      const next = onto ? movedProjects(order, moving, onto) : null;
      if (!next) return;
      app.bridge
        .command({ ChangePreference: { ProjectOrder: next } })
        .catch((error) => app.actions.tell(errorText(error), true));
    },
  };
}
