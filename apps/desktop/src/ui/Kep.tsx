// Kep, Muster's sheepdog: how the flock stands, in one face (design system: Kep).

import kepPortrait from "../assets/kep.svg";
import { type PartProps, usePart } from "./part";

export type Mood = "barking" | "glad" | "watching" | "asleep" | "away";

const FACES: Record<Mood, { face: string; says: string }> = {
  barking: { face: "▲•ᴥ•▲!", says: "Something asks for you" },
  glad: { face: "▼^ᴥ^▼", says: "A session just finished" },
  watching: { face: "▼•ᴥ•▼", says: "Kep is watching the flock" },
  asleep: { face: "▼-ᴥ-▼ᶻ", says: "Nothing works or waits" },
  away: { face: "▼;ᴥ;▼", says: "The daemon is out of reach" },
};

/** Kep's face for `mood`: in the sidebar's top, or `large` on a screen of its own (the daemon away). */
export function KepFace({
  mood,
  size = "small",
  ...props
}: PartProps<"span"> & { mood: Mood; size?: "small" | "large" }) {
  const { face, says } = FACES[mood];
  return usePart("span", props, {
    className: `mu-kep mu-kep--${mood} mu-kep--${size}`,
    title: says,
    role: "img",
    "aria-label": says,
    children: face,
  });
}

/** The 24 by 18 pixel portrait, for the welcome and empty screens. */
export function KepPortrait(props: PartProps<"img">) {
  return usePart("img", props, {
    className: "mu-kep-portrait",
    src: kepPortrait,
    alt: "Kep, Muster's sheepdog",
  });
}
