// The window's bottom edge: the selected thing's keys, Muster's notes, and
// the way to every key (design system: StatusBar).

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
      {note ? (
        <span className={`mu-note ${note.error ? "mu-note--error" : ""}`}>{note.text}</span>
      ) : null}
      {hints.map((hint) => (
        <Hint key={hint.key} keys={hint.key} does={hint.does} />
      ))}
      <span className="mu-spacer" />
      {help !== "none" ? <Hint keys={help} does="every key" /> : null}
      {version ? <span>Muster {version}</span> : null}
    </footer>
  );
}
