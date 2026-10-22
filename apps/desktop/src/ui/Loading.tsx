// What the window shows while something it asked for is on its way
// (design system: Loading). One look for every wait, so a wait reads the
// same wherever it is:
//
//   <Loading label="Fetching its tickets" />           a place with nothing yet: centred in it
//   <Loading label="Asking GitHub" layout="inline" />  a wait inside a line, a field or a dialog
//   <LoadingBar label="Fetching its tickets again" />  what is shown is being fetched again:
//                                                      a thin bar along its top, what is shown kept
//
// The spinner and the bar are CSS alone, so no script wakes for them, and
// they stand still while the window cannot be seen (ARC-25) and under
// Reduce motion.

import { Spinner } from "./controls";

export function Loading({
  label,
  layout = "block",
}: {
  /** What it waits for, said once and to a screen reader: "Fetching its tickets". */
  label: string;
  layout?: "block" | "inline";
}) {
  return (
    <div
      className={`mu-loading mu-loading--${layout}`}
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={`${label}…`}
    >
      <Spinner />
      <span>{label}…</span>
    </div>
  );
}

/** Along the top of what is shown while it is fetched again: shown at the top of its nearest positioned box. */
export function LoadingBar({ label }: { label: string }) {
  return (
    <div className="mu-loading-bar" role="status" aria-busy="true" aria-label={`${label}…`}>
      <span />
    </div>
  );
}
