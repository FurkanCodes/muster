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

import { type PartProps, usePart } from "./part";

export type BannerTone = "waiting" | "overlap" | "away" | "note";

export function Banner({
  tone = "waiting",
  children,
  ...props
}: PartProps<"div"> & { tone?: BannerTone }) {
  return usePart("div", props, {
    className: `mu-banner mu-banner--${tone}`,
    role: "status",
    children: (
      <>
        {tone === "note" ? null : <span className="mu-banner-mark" aria-hidden="true" />}
        {children}
      </>
    ),
  });
}

/** The words that lead it, in its tone's colour: "Needs you:", "Overlap". */
export function BannerLead(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-banner-lead" });
}

/** Its text on one line, cut where it runs out of room; all of it in its tooltip. */
export function BannerText(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-banner-text" });
}

/** Its text as long as it needs, wrapping. */
export function BannerBody(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-banner-body" });
}
