// Something chosen or not among several, each on its own (design system:
// Checkbox): a small box, ticked while chosen. Its label names what it
// chooses, for the person's screen reader and its tooltip. A click ticks
// it and leaves the keys where they were (a list's ⏎ stays the list's);
// ⇥ still reaches it.
//
//   <Checkbox checked={given} label="Give the agent mira's comment" onChange={toggle} />

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
    // biome-ignore lint/a11y/useSemanticElements: a button styled as the design system's box, as Switch is.
    <button
      type="button"
      role="checkbox"
      className="mu-checkbox"
      aria-checked={checked}
      aria-label={label}
      title={label}
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => {
        event.stopPropagation();
        onChange(!checked);
      }}
    />
  );
}
