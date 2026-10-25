// The list's lines and selection: the model's projects and sessions, the
// folds (the window's own, kept per project across restarts), and the line
// selected (`ui.selected.all`). The selection only ever rests on a line
// drawn: folding the project it is under takes it there, and one selected
// for the person (from a ticket, the palette, the notch) unfolds what hides
// it (`app::fold`).

import { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import { useActions, useApp, useDaemon, useUi } from "../../app/context";
import { readFolds, writeFolds } from "./folds";
import {
  allRows,
  foldOf,
  keptFolds,
  selectionAfterFold,
  shownRows,
  type TreeRow,
  toggled,
  unfoldedTo,
} from "./tree";

/** The rows as strings, so the list re-renders only when a line comes, goes or moves. */
function encoded(row: TreeRow): string {
  return [row.kind, row.id, row.project, row.space, row.workspace ?? "", row.place ? "1" : ""].join(
    " ",
  );
}

function decoded(text: string): TreeRow {
  const [kind = "project", id = "", project = "", space = "", workspace = "", place = ""] =
    text.split(" ");
  return {
    kind: kind as TreeRow["kind"],
    id,
    project,
    space,
    workspace: workspace || null,
    ...(place ? { place: true as const } : {}),
  };
}

export function useTree() {
  const app = useApp();
  const actions = useActions();
  const chosen = useUi((state) => state.selected.all);
  const loaded = useDaemon((state) => state.loaded);
  const space = useUi((state) => state.space);
  const lines = useDaemon(useShallow((state) => allRows(state, space).map(encoded)));
  const rows = lines.map(decoded);
  const [folded, setFolded] = useState<ReadonlySet<string>>(readFolds);
  const shown = shownRows(rows, folded);
  // Where the selection was, so a line that goes leaves it on the one in its place.
  const [lastAt, setLastAt] = useState(-1);

  const at = shown.findIndex((row) => row.id === chosen);
  const hidden = at < 0 ? rows.find((row) => row.id === chosen) : undefined;
  let selected: TreeRow | undefined = shown[at];
  if (!selected && !hidden) {
    // Gone: the line now in its place; at first, the first session.
    selected =
      lastAt < 0
        ? (shown.find((row) => row.kind === "session") ?? shown[0])
        : shown[Math.min(lastAt, shown.length - 1)];
  }
  const selectedAt = selected ? shown.indexOf(selected) : -1;

  const keep = (next: ReadonlySet<string>) => {
    setFolded(next);
    writeFolds(app.daemon.store.getState(), next);
  };

  // A line selected for the person unfolds what hides it.
  useEffect(() => {
    if (hidden) keep(unfoldedTo(folded, hidden));
  });

  // Folds of lines that are gone, or have nothing left to hide, are forgotten.
  useEffect(() => {
    if (!loaded) return;
    const kept = keptFolds(app.daemon.store.getState(), folded);
    if (kept !== folded) keep(kept);
  });

  const selectedId = selected?.id;
  useEffect(() => {
    if (selectedAt >= 0) setLastAt(selectedAt);
    if (selectedId && selectedId !== chosen) actions.select("all", selectedId);
  }, [actions, selectedId, selectedAt, chosen]);

  return {
    rows: shown,
    folded,
    selected,
    /** Folds or unfolds a space's or a project's line; on a line under one, the one it is under. */
    fold(id: string) {
      const row = rows.find((one) => one.id === id);
      if (!row) return;
      const target = foldOf(row);
      if (!target) return;
      keep(toggled(folded, target));
      const after = selectionAfterFold(target, selected);
      if (after && after !== selected?.id) actions.select("all", after);
    },
    pick(id: string) {
      actions.select("all", id);
    },
  };
}

export type Tree = ReturnType<typeof useTree>;
