// A group of things under a quiet heading (design system: Section): the
// heading names the region for a screen reader, wired by the section itself.
//
//   <Section>
//     <SectionTitle>Worktrees</SectionTitle>
//     …
//   </Section>

import { createContext, use, useId } from "react";
import { type PartProps, usePart } from "./part";

const TitleId = createContext<string | undefined>(undefined);

export function Section(props: PartProps<"section">) {
  const id = useId();
  const section = usePart("section", props, { className: "mu-section", "aria-labelledby": id });
  return <TitleId value={id}>{section}</TitleId>;
}

/** The section's heading: 12px, quiet, the region's name. */
export function SectionTitle(props: PartProps<"h3">) {
  const id = use(TitleId);
  return usePart("h3", props, { className: "mu-section-title", id });
}
