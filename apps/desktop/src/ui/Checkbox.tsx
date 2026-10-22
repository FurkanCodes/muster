// Something chosen or not among several, each on its own (design system:
// Checkbox): a small box, ticked while chosen. Its label names what it
// chooses, for the person's screen reader and its tooltip. A click ticks
// it and leaves the keys where they were (a list's ⏎ stays the list's);
// ⇥ still reaches it. On Base UI's checkbox, drawn as a button.
//
//   <Checkbox checked={given} label="Give the agent mira's comment" onChange={toggle} />

import { Checkbox as Base } from "@base-ui/react/checkbox";
import "./checkbox.css";

export function Checkbox({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Base.Root
      nativeButton
      render={<button type="button" />}
      className="mu-checkbox"
      checked={checked}
      onCheckedChange={(next) => onChange(next)}
      aria-label={label}
      title={label}
      onMouseDown={(event) => event.preventDefault()}
      // The box's own: what it sits in hears no click.
      onClick={(event) => event.stopPropagation()}
    />
  );
}
