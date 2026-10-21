// The window's bottom edge: the selected thing's keys, Muster's notes, and
// the way to every key (design system: StatusBar). The note is said to a
// screen reader as it comes: its place is a live region always there, so a
// note put in it is announced, and the note shown is the one said, never a
// hidden copy of it.

import { useKeyLabel } from "../keys/useKeymap";
import { Hint } from "../ui/controls";
import { useDaemon, useUi } from "./context";

export function StatusBar() {
  const note = useUi((state) => state.note);
  const hints = useUi((state) => state.hints);
  const version = useDaemon((state) => (state.link.state === "up" ? state.link.version : null));
  // The person's key for every key, as they set it; none where they took it away.
  const help = useKeyLabel("help");
  return (
    <footer className="mu-statusbar app-statusbar">
      <span
        data-said="note"
        aria-live={note?.error ? "assertive" : "polite"}
        aria-atomic="true"
        className={note ? `mu-note ${note.error ? "mu-note--error" : ""}` : "app-statusbar-said"}
      >
        {note?.text}
      </span>
      {hints.map((hint) => (
        <Hint key={hint.key} keys={hint.key} does={hint.does} />
      ))}
      <span className="mu-spacer" />
      {help !== "none" ? <Hint keys={help} does="every key" /> : null}
      {version ? <span>Muster {version}</span> : null}
    </footer>
  );
}
