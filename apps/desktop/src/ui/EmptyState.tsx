// What a place says while it has nothing to show (design system: EmptyState):
// Kep or nothing, a title, a line of text and what can be done, centred.
//
//   <EmptyState role="status">
//     <EmptyStateTitle>No projects yet</EmptyStateTitle>
//     <EmptyStateText>Each project you add shows here.</EmptyStateText>
//   </EmptyState>

import type { AriaRole, ReactNode } from "react";
import { cn } from "../lib/cn";

export function EmptyState({
  role,
  className,
  children,
}: {
  role?: AriaRole;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("mu-empty", className)} role={role}>
      {children}
    </div>
  );
}

/** Its title: a heading of the window's (`level` 1) or of a pane's (2, the default). */
export function EmptyStateTitle({
  level = 2,
  className,
  children,
}: {
  level?: 1 | 2;
  className?: string;
  children: ReactNode;
}) {
  const Heading = level === 1 ? "h1" : "h2";
  return <Heading className={cn("mu-empty-title", className)}>{children}</Heading>;
}

export function EmptyStateText({ children }: { children: ReactNode }) {
  return <p className="mu-empty-text">{children}</p>;
}

export function EmptyStateActions({ children }: { children: ReactNode }) {
  return <div className="mu-empty-actions">{children}</div>;
}
