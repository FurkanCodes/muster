// One line over the terminal saying what the session waits for, while it
// waits (design system: NeedsYouBanner). It never offers the agent's
// choices: the person answers in the terminal.

import { useNow } from "../app/clock";
import { useDaemon } from "../app/context";
import type { SessionId } from "../ipc/generated/SessionId";
import { ago } from "../lib/format";
import { doing } from "../lib/status";
import { Hint } from "../ui/controls";
import { StatusMark } from "../ui/status";
import { plainText } from "./question";

export function NeedsYouBanner({ session }: { session: SessionId }) {
  const waits = useDaemon((state) => {
    const stored = state.sessions[session];
    return stored?.status.state === "needs_you" ? plainText(doing(stored)) : null;
  });
  const since = useDaemon((state) => state.sessions[session]?.status_since ?? 0);
  if (waits === null) return null;
  return (
    <div className="mu-banner session-banner" role="status">
      <StatusMark kind="waiting" />
      <span className="mu-banner-text mu-cut" title={waits}>
        <span className="mu-banner-lead">Needs you:</span> {waits}
      </span>
      <Since since={since} />
      <Hint keys="⏎" does="answer in the terminal" />
    </div>
  );
}

/** How long it has waited; only this re-renders as the clock ticks. */
function Since({ since }: { since: number }) {
  const now = useNow();
  return <span className="mu-meta">{ago(now - since)}</span>;
}
