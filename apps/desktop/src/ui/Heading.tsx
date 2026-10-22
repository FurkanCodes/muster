// A place's own heading (design system: Heading): its title, a line under it
// saying what it is, and what can be done with all of it at its end.
//
//   <Heading>
//     <HeadingTitle>Plugins</HeadingTitle>
//     <HeadingLead>Muster's own and the ones you added.</HeadingLead>
//     <HeadingActions><Button>Add a plugin</Button></HeadingActions>
//   </Heading>

import { type PartProps, usePart } from "./part";

export function Heading(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-heading" });
}

/** Its title: the window's heading (`level` 1, a column's) or a page's (2, the default). */
export function HeadingTitle({ level = 2, ...props }: PartProps<"h2"> & { level?: 1 | 2 }) {
  return usePart(level === 1 ? "h1" : "h2", props, { className: "mu-heading-title" });
}

export function HeadingLead(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-heading-lead" });
}

export function HeadingActions(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-heading-actions" });
}
