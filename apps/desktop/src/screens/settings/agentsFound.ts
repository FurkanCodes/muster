// Where each coding agent is installed and the version it says it is, as
// the daemon finds them (`Command::Agents`, protocol 45): asked as the
// Agents page is shown and again on the person's word, never polled
// (ARC-25). A daemon older than 45 is never asked: there, where each is
// installed is not known, which the page says rather than an error.

import { useEffect, useState } from "react";
import { useBridge, useDaemon } from "../../app/context";
import { replyAs } from "../../ipc/ask";
import type { AgentFound } from "../../ipc/generated/AgentFound";
import { errorText } from "../../lib/text";

export type Found =
  | { state: "asking" }
  | { state: "found"; agents: Readonly<Record<string, AgentFound>> }
  /** The daemon could not say: not known. */
  | { state: "not-known"; why: string };

/** What the daemon says of each agent; asked again each time `again` changes. */
export function useAgentsFound(again: number): Found {
  const bridge = useBridge();
  const protocol = useDaemon((state) => (state.link.state === "up" ? state.link.protocol : null));
  const [found, setFound] = useState<Found>({ state: "asking" });
  // biome-ignore lint/correctness/useExhaustiveDependencies: asked again each time `again` changes, on the person's word.
  useEffect(() => {
    if (protocol === null) return;
    let current = true;
    setFound({ state: "asking" });
    bridge.command("Agents").then(
      (reply) => {
        if (!current) return;
        const agents = replyAs(reply, "Agents");
        if (agents)
          setFound({
            state: "found",
            agents: Object.fromEntries(agents.map((agent) => [agent.harness, agent])),
          });
        else setFound({ state: "not-known", why: "The daemon answered something else." });
      },
      (error: unknown) => {
        if (current)
          setFound({
            state: "not-known",
            why: errorText(error),
          });
      },
    );
    return () => {
      current = false;
    };
  }, [bridge, protocol, again]);
  return found;
}
