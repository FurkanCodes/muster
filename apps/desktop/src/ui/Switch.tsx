// A setting that is on or off and takes effect at once (design system:
// Switch), on Base UI's switch, drawn as a button. Busy while its change is
// on its way: it takes no second press.

import { Switch as Base } from "@base-ui/react/switch";
import { nativeButton } from "./part";

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
    <Base.Root
      nativeButton
      render={nativeButton}
      className="mu-switch"
      checked={on}
      onCheckedChange={(next) => {
        if (!busy && !disabled) onChange(next);
      }}
      aria-label={label}
      aria-busy={busy || undefined}
      aria-disabled={disabled || undefined}
      title={title}
    />
  );
}
