import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useKeys } from "../app/keys";
import type { FileDiff } from "../ipc/generated/FileDiff";
import { type DiffLine, isBinary, LINE, lineAt, parseDiff } from "./diff";
import { type PartProps, usePart } from "./part";
import "./diff-view.css";

/** Lines drawn beyond the edges of the view, so a scroll shows no gap. */
const OVERSCAN = 10;

/** The scrolling keys of the diff while it has the keys, in lines (a page is the view less one). */
const BY_LINES: Record<string, number> = { ArrowDown: 1, j: 1, ArrowUp: -1, k: -1 };

/**
 * One file's or one turn's diff, as git gives it (design system: DiffView):
 * line numbers, added and removed lines in the diff colours, hunk heads.
 * Only the lines in sight are drawn, so a diff of thousands of lines
 * scrolls without stalling the window (ARC-25).
 */
export function DiffView({
  diff,
  head,
  takeFocus = false,
  onLeave,
  onFocus,
}: {
  diff: FileDiff;
  /** The head's own parts: the kind, the path or turn, the stat, the part it is from. */
  head: ReactNode;
  /** Gives the diff the keys as it opens. */
  takeFocus?: boolean;
  /** ⎋: back to the list. */
  onLeave(): void;
  onFocus?(): void;
}) {
  const lines = useMemo(() => parseDiff(diff.text), [diff.text]);
  const binary = useMemo(() => isBinary(diff.text, lines), [diff.text, lines]);
  const body = useRef<HTMLElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(360);
  const focusFirst = useRef(takeFocus);
  useEffect(() => {
    const element = body.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight || 360));
    observer.observe(element);
    if (focusFirst.current) element.focus();
    return () => observer.disconnect();
  }, []);
  const scroll = (to: number) => body.current?.scrollTo({ top: to });
  const hunk = (direction: 1 | -1) => {
    const at = body.current?.scrollTop ?? 0;
    const hunks = lines.filter((line) => line.kind === "hunk");
    const target =
      direction > 0
        ? hunks.find((line) => line.offset > at + 1)
        : hunks.findLast((line) => line.offset < at - 1);
    if (target) scroll(target.offset);
  };
  useKeys({ n: () => hunk(1), p: () => hunk(-1) });
  const last = lines.at(-1);
  const total = last ? last.offset + last.height : 0;
  const start = Math.max(0, lineAt(lines, top) - OVERSCAN);
  const end = Math.min(lines.length, lineAt(lines, top + height) + OVERSCAN + 1);
  const width = useMemo(
    () => lines.reduce((widest, line) => Math.max(widest, line.text.length), 0),
    [lines],
  );
  const keys = (event: React.KeyboardEvent<HTMLElement>) => {
    const element = body.current;
    if (!element) return;
    const page = Math.max(LINE, height - LINE);
    if (event.key in BY_LINES) scroll(element.scrollTop + (BY_LINES[event.key] ?? 0) * LINE);
    else if (event.key === "PageDown" || event.key === " ") scroll(element.scrollTop + page);
    else if (event.key === "PageUp") scroll(element.scrollTop - page);
    else if (event.key === "Home" || event.key === "g") scroll(0);
    else if (event.key === "End" || event.key === "G") scroll(total);
    else if (event.key === "Escape") onLeave();
    else return;
    event.preventDefault();
    event.stopPropagation();
  };
  return (
    <DiffFrame>
      <DiffHead>{head}</DiffHead>
      <section
        ref={body}
        className="mu-diffview-body ui-diff-body"
        aria-label="Diff"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: the diff scrolls by keys, so it takes the focus (design system: DiffView).
        tabIndex={0}
        onKeyDown={keys}
        onFocus={onFocus}
        onScroll={(event) => setTop(event.currentTarget.scrollTop)}
      >
        {binary ? (
          <p className="mu-quiet">binary: git shows no lines for it.</p>
        ) : diff.text === "" ? (
          <p className="mu-quiet">Git shows no difference in its text.</p>
        ) : (
          <div style={{ minWidth: `calc(100px + ${width}ch)` }}>
            <div aria-hidden="true" style={{ height: lines[start]?.offset ?? 0 }} />
            {lines.slice(start, end).map((line) => (
              <Line key={line.offset} line={line} />
            ))}
            <div
              aria-hidden="true"
              style={{ height: Math.max(0, total - (lines[end]?.offset ?? total)) }}
            />
          </div>
        )}
        {diff.cut ? (
          <p className="mu-quiet">… cut here: the rest is longer than Muster shows.</p>
        ) : null}
      </section>
    </DiffFrame>
  );
}

function Line({ line }: { line: DiffLine }) {
  if (line.kind === "hunk") return <div className="mu-dl-hunk">{line.text}</div>;
  const tone = line.kind === "add" ? "mu-dl--add" : line.kind === "del" ? "mu-dl--del" : "";
  return (
    <div className={`mu-dl ${tone}`}>
      <span>{line.old}</span>
      <span>{line.next}</span>
      <span className="mu-dl-sign">
        {line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}
      </span>
      <span className={line.kind === "header" ? "mu-quiet" : undefined}>{line.text}</span>
    </div>
  );
}

/** A diff's frame: its head, then its lines or what it waits for; `className` for where it stands. */
export function DiffFrame(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-diffview ui-diff" });
}

/** What the diff is of: its file or its turn, and what it comes to. */
export function DiffHead(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-diffview-head" });
}

/** In place of the lines: what the diff waits for, or why it is not there. */
export function DiffWaiting(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-diffview-body ui-diff-body" });
}
