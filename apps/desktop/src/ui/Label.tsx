// A small quiet tag beside a name (design system: Label): a ticket's labels,
// "built in", "git", a key that is not yet set.
//
//   <Labels>{labels.map((label) => <Label key={label}>{label}</Label>)}</Labels>

import type { ReactNode } from "react";

export function Label({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <span className="mu-label" title={title}>
      {children}
    </span>
  );
}

/** Several labels in a row, wrapping. */
export function Labels({ children }: { children: ReactNode }) {
  return <span className="mu-labels">{children}</span>;
}
