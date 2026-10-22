// A strip saying something the person should know now (design system:
// NeedsYouBanner). Its tone draws its own mark, so no banner carries a mark
// of its own making:
//
//   waiting  something asks, or does not read (the waiting dot)
//   overlap  work meets other work (a dot in the working colour)
//   away     the daemon is out of reach (a ring in the working colour)
//   note     a word on what follows, no mark
//
//   <Banner tone="overlap">
//     <BannerText><BannerLead>Overlap</BannerLead> with feature/x: both change a.rs</BannerText>
//     <Button size="sm">See its changes</Button>
//   </Banner>

import type { AriaRole, ReactNode } from "react";
import { cn } from "../lib/cn";

export type BannerTone = "waiting" | "overlap" | "away" | "note";

export function Banner({
  tone = "waiting",
  role = "status",
  className,
  children,
}: {
  tone?: BannerTone;
  role?: AriaRole;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("mu-banner", `mu-banner--${tone}`, className)} role={role}>
      {tone === "note" ? null : <span className="mu-banner-mark" aria-hidden="true" />}
      {children}
    </div>
  );
}

/** The words that lead it, in its tone's colour: "Needs you:", "Overlap". */
export function BannerLead({ children }: { children: ReactNode }) {
  return <span className="mu-banner-lead">{children}</span>;
}

/** Its text on one line, cut where it runs out of room; all of it in its tooltip. */
export function BannerText({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <span className="mu-banner-text" title={title}>
      {children}
    </span>
  );
}

/** Its text as long as it needs, wrapping. */
export function BannerBody({ children }: { children: ReactNode }) {
  return <span className="mu-banner-body">{children}</span>;
}
