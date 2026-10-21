// A list of changed files that stays smooth however long it is (ARC-25):
// only the rows in sight are drawn, between two spacers as tall as the rows
// left out, as the diff view draws its lines. Every row is one height, so
// where a file is needs no measuring; the list scrolls to the selected file
// whenever the selection moves out of sight, as the TUI keeps it in view
// (app::changes::keep_selected_in_view).

import { type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { FileChange } from "../../ipc/generated/FileChange";
import { FileRow } from "../../ui/FileRow";

/** One row and the space under it, in pixels: FileRow's `size-control-sm` and `space-2` (changes.css). */
export const ROW = 26;
/** Rows drawn beyond the edges of the list, so a scroll shows no gap. */
const OVERSCAN = 8;
/** The list's height until it is measured (and where nothing is laid out, as in tests). */
const UNMEASURED = 640;

/** Where the list scrolls to from `top` so row `at` is in sight among `height` pixels. */
export function inSight(top: number, height: number, at: number): number {
  if (at < 0) return top;
  const from = at * ROW;
  if (from < top) return from;
  return from + ROW > top + height ? from + ROW - height : top;
}

export function FileList({
  files,
  folder,
  label,
  rows,
  selected,
  onSelect,
  onOpen,
  onDrawn,
}: {
  files: readonly FileChange[];
  folder: string;
  label: string;
  /** The list: the keys move through it, and it scrolls. */
  rows: RefObject<HTMLDivElement | null>;
  selected: string | null;
  onSelect(path: string): void;
  onOpen(path: string): void;
  /** Called once the rows in sight are drawn, after each change. */
  onDrawn?(): void;
}) {
  const at = selected === null ? -1 : files.findIndex((file) => file.path === selected);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(UNMEASURED);
  // The selected file is brought into sight as it is chosen, the list's own
  // scroll first and what is drawn after it, so the keys can focus it at once.
  const followed = useRef<number | null>(null);
  useLayoutEffect(() => {
    const box = rows.current;
    if (!box || followed.current === at) return;
    followed.current = at;
    const next = inSight(box.scrollTop, box.clientHeight || UNMEASURED, at);
    if (box.scrollTop !== next) box.scrollTop = next;
    setTop(next);
  }, [rows, at]);
  // Each time rows are drawn, the view may give the selected one the focus.
  useLayoutEffect(() => onDrawn?.());
  useEffect(() => {
    const box = rows.current;
    if (!box) return;
    const observer = new ResizeObserver(() => setHeight(box.clientHeight || UNMEASURED));
    observer.observe(box);
    return () => observer.disconnect();
  }, [rows]);
  const start = Math.max(0, Math.floor(top / ROW) - OVERSCAN);
  const end = Math.min(files.length, Math.ceil((top + height) / ROW) + OVERSCAN);
  return (
    <div
      ref={rows}
      role="listbox"
      aria-label={label}
      className="changes-list changes-files"
      // Read as the update is applied: a scroll's update can come after a
      // key's that moved the list since, and must not put it back.
      onScroll={() => setTop((was) => rows.current?.scrollTop ?? was)}
    >
      <div aria-hidden="true" style={{ height: start * ROW }} />
      {files.slice(start, end).map((file) => (
        <FileRow
          key={file.path}
          file={file}
          folder={folder}
          selected={selected === file.path}
          onSelect={() => onSelect(file.path)}
          onOpen={() => onOpen(file.path)}
        />
      ))}
      <div aria-hidden="true" style={{ height: (files.length - end) * ROW }} />
    </div>
  );
}
