// The window's bottom edge: where the keys go, the selected thing's keys,
// Muster's notes, and the way to every key (design system: StatusBar); at
// its right, what the session shown runs on and uses, the flock and the
// machine's memory. The note is said to a screen reader as it comes: its
// place is a live region always there, so a note put in it is announced,
// and the note shown is the one said, never a hidden copy of it. While a
// terminal has the keys it says so for as long as it has them, never
// longer: it is the window's state (`keysIn`), not a note that outlives it.

import type { SessionId } from "../ipc/generated/SessionId";
import { useKeyLabel } from "../keys/useKeymap";
import { agentOf } from "../lib/agents";
import { spend } from "../lib/format";
import { standsBeside } from "../state/selectors";
import { isList } from "../state/ui";
import { Hint, Spacer } from "../ui/controls";
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

/** The session beside the list, where one is selected there and shown. */
function useShownSession(): SessionId | null {
  const id = useUi((state) =>
    isList(state.screen) ? (state.selected[state.screen] ?? null) : null,
  );
  return useDaemon((state) => (id && state.sessions[id] ? id : null));
}

/** "OpenCode · $0.01, 36k tokens · 212 MB, 3% CPU": its agent, what it has spent, what it uses now. */
function SessionFacts({ session }: { session: SessionId }) {
  const harness = useDaemon((state) => state.sessions[session]?.harness ?? "");
  const tokens = useDaemon((state) => state.sessions[session]?.spend?.tokens ?? null);
  const cost = useDaemon((state) => state.sessions[session]?.spend?.cost_micros ?? null);
  const usage = useDaemon((state) => {
    const found = state.resources?.sessions.find((measured) => measured.session === session);
    return found
      ? `${Math.round(found.memory_bytes / 1e6)} MB, ${Math.round(found.cpu_permille / 10)}% CPU`
      : null;
  });
  const facts = [agentOf(harness).name, tokens === null ? null : spend(tokens, cost), usage];
  return <span className="app-statusbar-facts">{facts.filter(Boolean).join(" · ")}</span>;
}

/** Working, waiting on the person, done: the flock at a glance on every screen. */
function Flock() {
  const counts = useDaemon((state) => state.derived.counts);
  return (
    <span
      className="mu-flock app-flock"
      role="img"
      aria-label={`${counts.working} working, ${counts.waiting} waiting on you, ${counts.done} done`}
    >
      <span className="app-flock-working">
        <span className="app-flock-mark" aria-hidden="true" />
        {counts.working}
      </span>
      <span className="app-flock-waiting">
        <span className="app-flock-mark" aria-hidden="true" />
        {counts.waiting}
      </span>
      <span className="app-flock-done">
        <span className="app-flock-mark" aria-hidden="true" />
        {counts.done}
      </span>
    </span>
  );
}

const CELLS = [0, 1, 2, 3, 4, 5, 6, 7] as const;

/** Lit cells of eight, and the meter's tone; one number re-renders it, not each report. */
function MemoryMeter() {
  const percent = useDaemon((state) => {
    const report = state.resources;
    if (!report || report.host_memory_total === 0) return null;
    return Math.round((report.host_memory_used / report.host_memory_total) * 100);
  });
  if (percent === null) return null;
  const lit = Math.round((percent / 100) * 8);
  const tone = percent > 90 ? "mu-meter--full" : percent > 70 ? "mu-meter--busy" : "";
  return (
    <span
      className={`mu-meter ${tone}`}
      role="img"
      aria-label={`Memory ${percent} percent used`}
      title={`Memory ${percent}%`}
    >
      {CELLS.map((cell) => (
        <span key={cell} className={cell < lit ? "is-on" : undefined} />
      ))}
    </span>
  );
}

export function StatusBar() {
  const told = useUi((state) => state.note);
  const typing = useTyping();
  const note = told ?? (typing ? { text: typing, error: false } : null);
  const hints = useUi((state) => state.hints);
  const session = useShownSession();
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
      <Spacer />
      {session ? <SessionFacts session={session} /> : null}
      <Flock />
      <MemoryMeter />
      {help !== "none" ? <Hint keys={help} does="every key" /> : null}
      {version ? <span>Muster {version}</span> : null}
    </footer>
  );
}
