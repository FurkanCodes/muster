// What the window shows while something it asked for is on its way
// (design system: Loading), one look for every wait: a line with a light
// running along it, broken in its middle by the word Loading, which the
// light reaches and lights as it passes behind it, going on after it.
//
//   <Loading label="Fetching its text" />                  a place with nothing yet: centred in it
//   <Loading label="Fetching its tickets" layout="rule" />  a rule across its place, as under a heading
//
// `label` says to a screen reader what it waits for; the eye reads Loading.
// It is CSS alone, so no script wakes for it, it stands still while the
// window cannot be seen (ARC-25) and under Reduce motion.

export function Loading({
  label,
  layout = "block",
}: {
  /** What it waits for, to a screen reader: "Fetching its tickets". */
  label: string;
  /** Centred in a place with nothing yet, or a rule across its place. */
  layout?: "block" | "rule";
}) {
  return (
    <div
      className={`mu-loading mu-loading--${layout}`}
      role="status"
      aria-live="polite"
      aria-busy="true"
      aria-label={`${label}…`}
    >
      <span className="mu-loading-line mu-loading-line--before" />
      <span className="mu-loading-word" aria-hidden="true">
        Loading
      </span>
      <span className="mu-loading-line mu-loading-line--after" />
    </div>
  );
}
