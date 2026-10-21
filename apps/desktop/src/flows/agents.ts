// The agents a dialog offers (design system: AgentChoice; the TUI's
// harness_choice.rs): Muster's own coding agents, then those the person
// added in Muster's own settings, then a shell; the one chosen last first.
// A tidy takes only agents that take a first prompt, never a shell.

import type { AddedAgent } from "../ipc/generated/AddedAgent";
import { agentOf } from "../lib/agents";
import type { AgentOption } from "../ui/AgentChoice";

const OWN = ["claude", "codex", "gemini", "opencode", "copilot"] as const;

export interface Offered extends AgentOption {
  /** It takes a first prompt: every agent of Muster's own, and one of the person's whose settings say how. */
  takesPrompt: boolean;
}

export function offered(
  added: readonly AddedAgent[],
  last: string,
  work: "session" | "tidy",
): Offered[] {
  const all: Offered[] = [
    ...OWN.map((id) => ({ id, name: agentOf(id).name, takesPrompt: true })),
    ...added.map((agent) => ({
      id: agent.name,
      name: agent.label,
      takesPrompt: agent.takes_prompt,
    })),
    { id: "shell", name: "A shell", takesPrompt: false },
  ];
  const fit = work === "session" ? all : all.filter((agent) => agent.takesPrompt);
  const chosen = fit.find((agent) => agent.id === last);
  return chosen ? [chosen, ...fit.filter((agent) => agent !== chosen)] : fit;
}

/** The agent `by` places on from `id`, going round. */
export function stepAgent(agents: readonly AgentOption[], id: string, by: 1 | -1): string {
  const at = agents.findIndex((agent) => agent.id === id);
  return agents[(Math.max(at, 0) + by + agents.length) % agents.length]?.id ?? id;
}
