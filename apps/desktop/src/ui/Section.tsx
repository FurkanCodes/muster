// A group of things under a quiet heading (design system: Section): the
// heading names the region for a screen reader, wired by the section itself.
//
//   <Section>
//     <SectionTitle>Worktrees</SectionTitle>
//     …
//   </Section>

import { createContext, type ReactNode, use, useId } from "react";
import { cn } from "../lib/cn";

const TitleId = createContext<string | undefined>(undefined);

export function Section({ className, children }: { className?: string; children: ReactNode }) {
  const id = useId();
  return (
    <TitleId value={id}>
      <section className={cn("mu-section", className)} aria-labelledby={id}>
        {children}
      </section>
    </TitleId>
  );
}

/** The section's heading: 12px, quiet, the region's name. */
export function SectionTitle({ className, children }: { className?: string; children: ReactNode }) {
  const id = use(TitleId);
  return (
    <h3 className={cn("mu-section-title", className)} id={id}>
      {children}
    </h3>
  );
}
