// The list itself (design system: SessionTree): each project's line and its
// sessions under it, as one ARIA tree whose selection the window's keys move
// (`aria-activedescendant`). However long it is, only the lines in sight are
// drawn (sight.ts, ARC-25): thousands of sessions scroll as smoothly as ten.

import { type ReactNode, useLayoutEffect, useRef, useState } from "react";
import { useDaemon } from "../../app/context";
import type { SessionActions } from "../../session/useSessionActions";
import { RenameSession } from "../../session/useSessionActions";
import { standsBeside } from "../../state/selectors";
import { List } from "../../ui/List";
import { drawn, estimate, inSight, placesOf, UNMEASURED } from "./sight";
import {
  BesideLine,
  type LineProps,
  ProjectLine,
  rowId,
  SessionLine,
  WorktreeLine,
} from "./TreeRows";
import type { TreeRow } from "./tree";

function SessionOrBeside({
  props,
  panelKeys,
}: {
  props: LineProps;
  panelKeys: Readonly<Record<string, string>>;
}) {
  const harness = useDaemon((state) => state.sessions[props.row.id]?.harness ?? "");
  if (!standsBeside({ harness })) return <SessionLine {...props} />;
  return <BesideLine {...props} panelKey={panelKeys[harness] ?? null} />;
}

export function SessionTree({
  rows,
  folded,
  selected,
  resting,
  renaming,
  panelKeys,
  onPick,
  onFold,
}: {
  rows: readonly TreeRow[];
  folded: ReadonlySet<string>;
  selected: string | undefined;
  resting: boolean;
  renaming: SessionActions | null;
  /** Each plugin's panel's key, by its harness, as its line ends. */
  panelKeys: Readonly<Record<string, string>>;
  onPick: (id: string) => void;
  onFold: (id: string) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  const lines = useDaemon((state) => state.preferences?.session_lines ?? "two");
  // Each line's height as drawn, by its id and what it was drawn as; the rest are estimated.
  const measured = useRef(new Map<string, number>());
  const [, remeasured] = useState(0);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(UNMEASURED);

  const keyOf = (row: TreeRow, first: boolean) => `${row.id} ${first} ${lines}`;
  const heights = rows.map(
    (row, at) => measured.current.get(keyOf(row, at === 0)) ?? estimate(row, at === 0, lines),
  );
  const places = placesOf(heights);
  const [start, end] = drawn(places, top, height);

  // The lines drawn are measured, and drawn again where one was not as estimated.
  useLayoutEffect(() => {
    const box = list.current;
    if (!box) return;
    let changed = false;
    for (const node of box.querySelectorAll<HTMLElement>("[data-line]")) {
      const at = Number(node.dataset.line);
      const row = rows[at];
      const tall = node.offsetHeight;
      if (!row || tall === 0) continue;
      const key = keyOf(row, at === 0);
      if (measured.current.get(key) !== tall) {
        measured.current.set(key, tall);
        changed = true;
      }
    }
    if (changed) remeasured((count) => count + 1);
  });

  useLayoutEffect(() => {
    const box = list.current;
    if (!box) return;
    const observer = new ResizeObserver(() => setHeight(box.clientHeight || UNMEASURED));
    observer.observe(box);
    return () => observer.disconnect();
  }, []);

  // Scrolled to as it is selected, drawn or not.
  const at = selected === undefined ? -1 : rows.findIndex((row) => row.id === selected);
  // The list's own scroll first and what is drawn after it, so an update a
  // scroll made earlier never puts it back.
  const followed = useRef<string | undefined>(undefined);
  const from = places[at] ?? 0;
  const to = places[at + 1] ?? 0;
  useLayoutEffect(() => {
    const box = list.current;
    if (!box || followed.current === selected || at < 0) return;
    followed.current = selected;
    const next = inSight(box.scrollTop, box.clientHeight || height, from, to);
    if (box.scrollTop !== next) box.scrollTop = next;
    setTop(next);
  }, [selected, at, from, to, height]);

  const draw = (row: TreeRow, index: number): ReactNode => {
    const props: LineProps = {
      row,
      level: row.kind === "project" ? 1 : 2,
      selected: row.id === selected,
      folded: folded.has(row.id),
      onPick,
      onFold,
    };
    let line: ReactNode;
    if (row.kind === "project") line = <ProjectLine {...props} />;
    else if (row.kind === "workspace") line = <WorktreeLine {...props} />;
    else if (renaming?.renaming === row.id) line = <RenameSession actions={renaming} />;
    else line = <SessionOrBeside props={props} panelKeys={panelKeys} />;
    return (
      <div
        key={row.id}
        className={`sessions-node ${index === 0 ? "" : row.kind === "project" ? "sessions-node--project" : "sessions-node--under"}`}
        role="none"
        data-line={index}
      >
        <div className="sessions-line">{line}</div>
      </div>
    );
  };

  return (
    <List
      ref={list}
      kind="tree"
      label="Projects and sessions"
      className="sessions-tree"
      activeId={at >= start && at < end && selected ? rowId(selected) : undefined}
      resting={resting}
      // It keeps its selected line in sight itself, drawn or not.
      sight="list"
      // Read as the update is applied: a scroll's update can come after a
      // key's that moved the list since, and must not put it back.
      onScroll={() => setTop((was) => list.current?.scrollTop ?? was)}
    >
      <div aria-hidden="true" style={{ height: places[start] ?? 0 }} />
      {rows.slice(start, end).map((row, offset) => draw(row, start + offset))}
      <div aria-hidden="true" style={{ height: (places.at(-1) ?? 0) - (places[end] ?? 0) }} />
    </List>
  );
}
