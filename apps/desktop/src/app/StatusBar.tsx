// The window's bottom edge: the selected thing's keys, Muster's notes, and
// the way to every key (design system: StatusBar).

import { Hint } from "../ui/controls";
import { useDaemon, useUi } from "./context";

export function StatusBar() {
  const note = useUi((state) => state.note);
  const hints = useUi((state) => state.hints);
  const version = useDaemon((state) => (state.link.state === "up" ? state.link.version : null));
  return (
    <footer className="mu-statusbar app-statusbar">
      {note ? (
        <span className={`mu-note ${note.error ? "mu-note--error" : ""}`}>{note.text}</span>
      ) : null}
      {hints.map((hint) => (
        <Hint key={hint.key} keys={hint.key} does={hint.does} />
      ))}
      <span className="mu-spacer" />
      <Hint keys="h" does="every key" />
      {version ? <span>Muster {version}</span> : null}
    </footer>
  );
}
