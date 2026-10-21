// A setting that is on or off and takes effect at once (design system:
// Switch). Busy while its change is on its way: it takes no second press.

export function Switch({
  on,
  label,
  onChange,
  busy = false,
  disabled = false,
  title,
}: {
  on: boolean;
  /** What it turns on, for the person's screen reader. */
  label: string;
  onChange: (on: boolean) => void;
  busy?: boolean;
  disabled?: boolean;
  /** Why it cannot be pressed, where it cannot. */
  title?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="mu-switch"
      aria-checked={on}
      aria-label={label}
      aria-busy={busy || undefined}
      aria-disabled={disabled || undefined}
      title={title}
      onClick={() => {
        if (!busy && !disabled) onChange(!on);
      }}
    />
  );
}
