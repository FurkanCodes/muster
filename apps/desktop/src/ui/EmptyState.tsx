// What a place says while it has nothing to show (design system: EmptyState):
// Kep or nothing, a title, a line of text and what can be done, centred.
//
//   <EmptyState role="status">
//     <EmptyStateTitle>No projects yet</EmptyStateTitle>
//     <EmptyStateText>Each project you add shows here.</EmptyStateText>
//   </EmptyState>

import { type PartProps, usePart } from "./part";

export function EmptyState(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-empty" });
}

/** Its title: a heading of the window's (`level` 1) or of a pane's (2, the default). */
export function EmptyStateTitle({ level = 2, ...props }: PartProps<"h2"> & { level?: 1 | 2 }) {
  return usePart(level === 1 ? "h1" : "h2", props, { className: "mu-empty-title" });
}

export function EmptyStateText(props: PartProps<"p">) {
  return usePart("p", props, { className: "mu-empty-text" });
}

export function EmptyStateActions(props: PartProps<"div">) {
  return usePart("div", props, { className: "mu-empty-actions" });
}
