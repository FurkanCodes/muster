import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useKeys } from "../app/keys";
import type { FileDiff } from "../ipc/generated/FileDiff";
import { lineAt, parseDiff } from "./diff";

export function DiffView({ diff, children }: { diff: FileDiff; children: ReactNode }) {
  const lines = useMemo(() => parseDiff(diff.text), [diff.text]);
  const viewport = useRef<HTMLElement>(null);
  const [top, setTop] = useState(0);
  const [height, setHeight] = useState(360);
  useEffect(() => {
    const element = viewport.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight || 360));
    observer.observe(element);
    element.focus();
    return () => observer.disconnect();
  }, []);
  const scroll = (to: number) => viewport.current?.scrollTo({ top: to });
  const hunk = (direction: number) => {
    const at = viewport.current?.scrollTop ?? 0;
    const hunks = lines.filter((line) => line.kind === "hunk");
    const target =
      direction > 0
        ? hunks.find((line) => line.offset > at + 1)
        : hunks.findLast((line) => line.offset < at - 1);
    if (target) scroll(target.offset);
  };
  useKeys({ n: () => hunk(1), p: () => hunk(-1) });
  const start = Math.max(0, lineAt(lines, top) - 10);
  const end = Math.min(lines.length, lineAt(lines, top + height) + 11);
  const last = lines.at(-1);
  const total = last ? last.offset + last.height : 0;
  const width = lines.reduce((max, line) => Math.max(max, line.text.length), 0);
  const keyboard = (event: React.KeyboardEvent<HTMLElement>) => {
    const element = viewport.current;
    if (!element) return;
    const by: Record<string, number> = {
      ArrowDown: 18,
      j: 18,
      ArrowUp: -18,
      k: -18,
      PageDown: height - 18,
      " ": height - 18,
      PageUp: -height + 18,
    };
    if (event.key in by) scroll(element.scrollTop + (by[event.key] ?? 0));
    else if (event.key === "Home" || event.key === "g") scroll(0);
    else if (event.key === "End" || event.key === "G") scroll(total);
    else return;
    event.preventDefault();
    event.stopPropagation();
  };
  return (
    <div className="mu-diffview changes-diff">
      <div className="mu-diffview-head">{children}</div>
      <section
        ref={viewport}
        className="mu-diffview-body changes-diff-body"
        aria-label="Diff"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: the scrollable diff needs focus for keyboard scrolling (ARC-24).
        tabIndex={0}
        onKeyDown={keyboard}
        onScroll={(event) => setTop(event.currentTarget.scrollTop)}
      >
        {diff.text === "" ? <p className="mu-meta">Git shows no difference in its text.</p> : null}
        <div style={{ minWidth: `calc(100px + ${width}ch)` }}>
          <div aria-hidden="true" style={{ height: lines[start]?.offset ?? 0 }} />
          {lines.slice(start, end).map((line) =>
            line.kind === "hunk" ? (
              <div key={line.offset} className="mu-dl-hunk">
                {line.text}
              </div>
            ) : (
              <div
                key={line.offset}
                className={`mu-dl ${line.kind === "add" ? "mu-dl--add" : line.kind === "del" ? "mu-dl--del" : ""}`}
              >
                <span>{line.old}</span>
                <span>{line.next}</span>
                <span className="mu-dl-sign">
                  {line.kind === "add" ? "+" : line.kind === "del" ? "−" : ""}
                </span>
                <span>{line.text}</span>
              </div>
            ),
          )}
          <div
            aria-hidden="true"
            style={{ height: Math.max(0, total - (lines[end]?.offset ?? total)) }}
          />
        </div>
        {diff.cut ? (
          <p className="mu-meta">… cut here: the rest is longer than Muster shows.</p>
        ) : null}
      </section>
    </div>
  );
}
