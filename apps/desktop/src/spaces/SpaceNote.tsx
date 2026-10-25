// The spaces a line shows in (`in Work, Home`), as the model notes them
// (`Derived::space_notes`): beside a project's name while everything is
// shown, and on a session waiting on the person, as alerts reach the person
// whichever space is shown. Nothing where it is in no space.

import { useDaemon, useUi } from "../app/context";

export function SpaceNote({ of, always = true }: { of: string; always?: boolean }) {
  const note = useDaemon((state) => state.derived.space_notes[of] ?? null);
  const everything = useUi((state) => state.space === null);
  if (!note || (!always && !everything)) return null;
  return <span className="mu-quiet mu-cut">{note}</span>;
}
