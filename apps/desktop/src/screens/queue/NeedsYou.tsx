import { useDaemon } from "../../app/context";
import { Placeholder } from "../../app/Placeholder";
import { SessionPane } from "../../session/SessionPane";

// The foundation's stand-in: the queue is built on feature/desktop-queue; the
// pane beside it shows the oldest session that needs the person, live.
export default function Screen() {
  const first = useDaemon((state) => state.derived.needs_you[0] ?? null);
  return (
    <>
      <Placeholder title="Needs you" branch="feature/desktop-queue" />
      {first ? <SessionPane session={first} /> : null}
    </>
  );
}
