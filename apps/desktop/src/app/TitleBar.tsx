// The window's top edge: Kep, the way to anything by typing, the flock and
// the machine's memory (design system: TitleBar).

import { Kbd } from "../ui/controls";
import { Icon } from "../ui/Icon";
import { KepFace, type Mood } from "../ui/Kep";
import { useActions, useDaemon } from "./context";

function useMood(): Mood {
  return useDaemon((state): Mood => {
    if (state.link.state === "down") return "away";
    if (state.derived.counts.waiting > 0) return "barking";
    if (state.derived.counts.working > 0 || state.link.state === "connecting") return "watching";
    return "asleep";
  });
}

/** Lit cells of eight, and the meter's tone; one number re-renders it, not each report. */
function useMemory(): number | null {
  return useDaemon((state) => {
    const report = state.resources;
    if (!report || report.host_memory_total === 0) return null;
    return Math.round((report.host_memory_used / report.host_memory_total) * 100);
  });
}

function MemoryMeter({ percent }: { percent: number }) {
  const lit = Math.round((percent / 100) * 8);
  const tone = percent > 90 ? "mu-meter--full" : percent > 70 ? "mu-meter--busy" : "";
  return (
    <div
      className={`mu-meter ${tone}`}
      role="img"
      aria-label={`Memory ${percent} percent used`}
      title={`Memory ${percent}%`}
    >
      {CELLS.map((cell) => (
        <span key={cell} className={cell < lit ? "is-on" : undefined} />
      ))}
    </div>
  );
}

const CELLS = [0, 1, 2, 3, 4, 5, 6, 7] as const;

function Flock() {
  const counts = useDaemon((state) => state.derived.counts);
  return (
    <div
      className="mu-flock"
      role="img"
      aria-label={`${counts.working} working, ${counts.waiting} waiting on you, ${counts.done} done`}
    >
      <span className="app-flock-working">
        <span className="mu-mono" aria-hidden="true">
          ⠿
        </span>
        {counts.working}
      </span>
      <span className="app-flock-waiting">
        <span className="mu-mono app-flock-triangle" aria-hidden="true">
          ▲
        </span>
        {counts.waiting}
      </span>
      <span className="app-flock-done">
        <span className="mu-mono" aria-hidden="true">
          ✓
        </span>
        {counts.done}
      </span>
    </div>
  );
}

export function TitleBar() {
  const actions = useActions();
  const mood = useMood();
  const memory = useMemory();
  const linked = useDaemon((state) => state.link.state === "up");
  return (
    <header className="mu-titlebar app-titlebar" data-tauri-drag-region>
      <KepFace mood={mood} />
      <span className="mu-spacer" data-tauri-drag-region />
      <button
        type="button"
        className="mu-command"
        onClick={() => actions.openPalette(true)}
        disabled={!linked}
      >
        <Icon name="search" />
        <span>{linked ? "Go to a session or run a command" : "Waiting for the daemon"}</span>
        {linked ? <Kbd>⌘K</Kbd> : null}
      </button>
      <span className="mu-spacer" data-tauri-drag-region />
      <Flock />
      {memory === null ? null : <MemoryMeter percent={memory} />}
    </header>
  );
}
