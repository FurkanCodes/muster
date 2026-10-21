// Kep, Muster's sheepdog: how the flock stands, in one face (design system: Kep).

import kepPortrait from "../assets/kep.svg";

export type Mood = "barking" | "glad" | "watching" | "asleep" | "away";

const FACES: Record<Mood, { face: string; says: string }> = {
  barking: { face: "▲•ᴥ•▲!", says: "Something asks for you" },
  glad: { face: "▼^ᴥ^▼", says: "A session just finished" },
  watching: { face: "▼•ᴥ•▼", says: "Kep is watching the flock" },
  asleep: { face: "▼-ᴥ-▼ᶻ", says: "Nothing works or waits" },
  away: { face: "▼;ᴥ;▼", says: "The daemon is out of reach" },
};

export function KepFace({ mood }: { mood: Mood }) {
  const { face, says } = FACES[mood];
  return (
    <span className={`mu-kep mu-kep--${mood}`} title={says} role="img" aria-label={says}>
      {face}
    </span>
  );
}

/** The 24 by 18 pixel portrait, for the welcome and empty screens. */
export function KepPortrait() {
  return <img className="mu-kep-portrait" src={kepPortrait} alt="Kep, Muster's sheepdog" />;
}
