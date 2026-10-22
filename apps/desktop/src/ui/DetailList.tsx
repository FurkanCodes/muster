// Facts as name and value, two columns (design system: DetailList).
//
//   <DetailList>
//     <Detail name="From">github.com/…</Detail>
//   </DetailList>

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function DetailList({ className, children }: { className?: string; children: ReactNode }) {
  return <dl className={cn("mu-details", className)}>{children}</dl>;
}

/** One fact: its name, then its value. */
export function Detail({ name, children }: { name: ReactNode; children: ReactNode }) {
  return (
    <>
      <dt>{name}</dt>
      <dd>{children}</dd>
    </>
  );
}
