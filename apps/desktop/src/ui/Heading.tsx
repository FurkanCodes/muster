// A place's own heading (design system: Heading): its title, a line under it
// saying what it is, and what can be done with all of it at its end.
//
//   <Heading>
//     <HeadingTitle>Plugins</HeadingTitle>
//     <HeadingLead>Muster's own and the ones you added.</HeadingLead>
//     <HeadingActions><Button>Add a plugin</Button></HeadingActions>
//   </Heading>

import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Heading({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cn("mu-heading", className)}>{children}</div>;
}

/** Its title: the window's heading (`level` 1, a column's) or a page's (2, the default). */
export function HeadingTitle({ level = 2, children }: { level?: 1 | 2; children: ReactNode }) {
  const Title = level === 1 ? "h1" : "h2";
  return <Title className="mu-heading-title">{children}</Title>;
}

export function HeadingLead({ children }: { children: ReactNode }) {
  return <span className="mu-heading-lead">{children}</span>;
}

export function HeadingActions({ children }: { children: ReactNode }) {
  return <div className="mu-heading-actions">{children}</div>;
}
