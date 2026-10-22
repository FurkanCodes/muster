// What the window shows while something it asked for is on its way
// (design system: Loading), one look for every wait: a line with a light
// running along it, broken in its middle by the word Loading, which the
// light reaches and lights as it passes behind it, going on after it.
//
//   <Loading />                                  Loading, centred in a place with nothing yet
//   <Loading text="Loading tickets" layout="rule" />  its own words, a rule across its place, as under a heading
//
// Its words are what a screen reader hears too; Loading where none are given.
// It is CSS alone, so no script wakes for it, it stands still while the
// window cannot be seen (ARC-25) and under Reduce motion. Every wait in the
// window is one: no screen draws a wait of its own.

import { type PartProps, usePart } from "./part";

export function Loading({
  text = "Loading",
  layout = "block",
  ...props
}: PartProps<"div"> & {
  /** Its words, in its middle: what it waits for, short ("Loading tickets"). */
  text?: string;
  /** Centred in a place with nothing yet, or a rule across its place. */
  layout?: "block" | "rule";
}) {
  return usePart("div", props, {
    className: `mu-loading mu-loading--${layout}`,
    role: "status",
    "aria-live": "polite",
    "aria-busy": true,
    "aria-label": text,
    children: (
      <>
        <span className="mu-loading-line mu-loading-line--before" />
        <span className="mu-loading-word" aria-hidden="true">
          {text}
        </span>
        <span className="mu-loading-line mu-loading-line--after" />
      </>
    ),
  });
}
