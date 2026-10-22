// A small quiet tag beside a name (design system: Label): a ticket's labels,
// "built in", "git", a key that is not yet set.
//
//   <Labels>{labels.map((label) => <Label key={label}>{label}</Label>)}</Labels>

import { type PartProps, usePart } from "./part";

export function Label(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-label" });
}

/** Several labels in a row, wrapping. */
export function Labels(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-labels" });
}
