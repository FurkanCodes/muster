// The daemon out of reach (design/screens/Away.html): the link starts it
// again and tries every half second on its own; the window says so, keeps
// what it last knew as it was, still, and says where to look meanwhile.
// The time out of reach moves with the window's one clock (`useNow`).

import { useNow } from "../app/clock";
import { useDaemon } from "../app/context";
import { ago } from "../lib/format";
import "./away.css";

function useAway(): { since: number; reason: string } | null {
  const since = useDaemon((state) => (state.link.state === "down" ? state.link.since : null));
  const reason = useDaemon((state) => (state.link.state === "down" ? state.link.reason : ""));
  return since === null ? null : { since, reason };
}

/** Above the window, while the daemon is out of reach. */
export function AwayBanner() {
  const away = useAway();
  const now = useNow();
  if (!away) return null;
  return (
    <div className="mu-banner mu-banner--away ui-flow-away" role="alert">
      <span className="mu-mark ui-flow-away-mark" aria-hidden="true">
        !
      </span>
      <span className="ui-flow-away-text">
        <span className="mu-banner-lead">Muster's daemon is out of reach.</span> Starting it again
        and trying every half second, {ago(now - away.since)} so far. What you see is as it was.
        {away.reason ? <span className="mu-quiet"> {away.reason}</span> : null}
      </span>
    </div>
  );
}

/** In place of a screen that has nothing yet to show as it was. */
export function Away() {
  const away = useAway();
  const at = away ? new Date(away.since).toLocaleTimeString([], { hour12: false }) : null;
  return (
    <div className="mu-empty">
      <span className="mu-kep mu-kep--away ui-flow-kep" role="img" aria-label="Kep, away">
        ▼;ᴥ;▼
      </span>
      <h2 className="mu-empty-title">Waiting for the daemon</h2>
      <p className="mu-empty-text">
        When it is back, your sessions are back with it: those in containers as they were, the rest
        through their own agent's resume. Closing this window changes nothing.
      </p>
      {away ? (
        <p className="mu-meta ui-flow-said">
          {away.reason ? `${away.reason.replace(/\.$/, "")}. ` : null}Out of reach since {at}. Its
          log is daemon.log in Muster's runtime folder.
        </p>
      ) : null}
    </div>
  );
}
