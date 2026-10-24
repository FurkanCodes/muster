// Where each coding agent is installed and the version it says it is, as
// the daemon finds them (`Command::Agents`): asked as the Agents page is
// shown, while the daemon is there, and again on the person's word, never
// polled (ARC-25).

import { useBridge, useDaemon } from "../../app/context";
import { ask } from "../../ipc/ask";
import type { AgentFound } from "../../ipc/generated/AgentFound";
import { type Asked, useAsked } from "../../ipc/useAsked";

/** Each agent found, by its harness; failed where the daemon could not say. */
export type Found = Asked<Readonly<Record<string, AgentFound>>>;

/** What the daemon says of each agent; asked again each time `again` changes. */
export function useAgentsFound(again: number): Found {
  const bridge = useBridge();
  const up = useDaemon((state) => state.link.state === "up");
  return useAsked(
    up
      ? async () =>
          Object.fromEntries(
            (await ask(bridge, "Agents", "Agents")).map((agent) => [agent.harness, agent]),
          )
      : null,
    String(again),
  );
}
