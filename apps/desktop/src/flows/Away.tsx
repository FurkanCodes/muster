// The daemon out of reach (design/screens/Away.html): the link starts it
// again and tries every half second on its own; the window says so, keeps
// what it last knew as it was, still, and says where to look meanwhile.
// The time out of reach moves with the window's one clock (`useNow`). The
// banner opens the daemon's log and tries at once, on the person's word.

import { useNow } from "../app/clock";
import { useActions, useBridge, useDaemon } from "../app/context";
import { ago } from "../lib/format";
import { Banner, BannerBody, BannerLead } from "../ui/Banner";
import { Button } from "../ui/controls";
import { EmptyState, EmptyStateText, EmptyStateTitle } from "../ui/EmptyState";
import { KepFace } from "../ui/Kep";
import { Loading } from "../ui/Loading";
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
  const bridge = useBridge();
  const actions = useActions();
  if (!away) return null;
  const showLog = () => {
    bridge.showDaemonLog().catch((error: unknown) => {
      actions.tell(error instanceof Error ? error.message : String(error), true);
    });
  };
  const tryNow = () => {
    void bridge.tryNow().catch(() => {});
  };
  return (
    <Banner tone="away" role="alert" className="ui-flow-away">
      <BannerBody>
        <BannerLead>Muster's daemon is out of reach.</BannerLead> Starting it again and trying every
        half second, {ago(now - away.since)} so far. What you see is as it was.
        {away.reason ? <span className="mu-quiet"> {away.reason}</span> : null}
      </BannerBody>
      <Button variant="quiet" size="sm" title="Open daemon.log" onClick={showLog}>
        Show its log
      </Button>
      <Button variant="quiet" size="sm" onClick={tryNow}>
        Try now
      </Button>
    </Banner>
  );
}

/** In place of a screen that has nothing yet to show as it was. */
export function Away() {
  const away = useAway();
  const at = away ? new Date(away.since).toLocaleTimeString([], { hour12: false }) : null;
  return (
    <EmptyState>
      <KepFace mood="away" size="large" />
      <EmptyStateTitle>Muster's daemon is away</EmptyStateTitle>
      <EmptyStateText>
        When it is back, your sessions are back with it: those in containers as they were, the rest
        through their own agent's resume. Closing this window changes nothing.
      </EmptyStateText>
      <Loading text="Waiting for the daemon" layout="rule" />
      {away ? (
        <p className="mu-meta ui-flow-said">
          {away.reason ? `${away.reason.replace(/\.$/, "")}. ` : null}Out of reach since {at}. Its
          log is daemon.log in Muster's runtime folder.
        </p>
      ) : null}
    </EmptyState>
  );
}
