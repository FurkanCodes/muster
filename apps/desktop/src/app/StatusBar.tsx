// The window's bottom edge: where the keys go, the selected thing's keys,
// Muster's notes, and the way to every key (design system: StatusBar). The
// note is said to a screen reader as it comes: its place is a live region
// always there, so a note put in it is announced, and the note shown is the
// one said, never a hidden copy of it. While a terminal has the keys it says
// so for as long as it has them, never longer: it is the window's state
// (`keysIn`), not a note that outlives it.

import { useKeyLabel } from "../keys/useKeymap";
import { agentOf } from "../lib/agents";
import { standsBeside } from "../state/selectors";
import { Hint } from "../ui/controls";
import { useDaemon, useUi } from "./context";

/** What the status bar says while a terminal has the keys, or null while Muster has them. */
function useTyping(): string | null {
  const typing = useUi((state) => (state.keysIn === "terminal" ? state.terminal : null));
  return useDaemon((state) => {
    const session = typing ? state.sessions[typing] : undefined;
    if (!session) return null;
    if (standsBeside(session)) return `Typing in ${session.title}. Every key goes to it.`;
    return `Typing in ${session.title}. Every key goes to ${agentOf(session.harness).name}.`;
  });
}

export function StatusBar() {
  const told = useUi((state) => state.note);
  const typing = useTyping();
  const note = told ?? (typing ? { text: typing, error: false } : null);
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
        data-typing={!told && typing ? true : undefined}
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
