// The coding agents Muster can start: its own, as ARCHITECTURE.md §7 says
// each is heard and carries on, and those the person added in Muster's own
// settings (`[agents.<name>]`), as the daemon reads them. Which can name
// chats is `muster_core::ask::built_in`'s: Claude Code and Codex, the
// others once each is checked.

import type { AddedAgent } from "../../ipc/generated/AddedAgent";

export interface OwnAgent {
  id: string;
  /** How Muster hears what it is doing. */
  status: string;
  /** Whether it carries on after a restart, from the id its reports carry. */
  resumes: boolean;
  /** Whether it can be asked one question, and so name chats. */
  names: boolean;
  /** How it is installed, where it is not found. */
  install: string;
}

export const OWN_AGENTS: readonly OwnAgent[] = [
  {
    id: "claude",
    status: "status from its hooks",
    resumes: true,
    names: true,
    install: "npm install -g @anthropic-ai/claude-code",
  },
  {
    id: "codex",
    status: "status from its turn notice and window title",
    resumes: true,
    names: true,
    install: "npm install -g @openai/codex",
  },
  {
    id: "gemini",
    status: "status from its hooks",
    resumes: true,
    names: false,
    install: "npm install -g @google/gemini-cli",
  },
  {
    id: "opencode",
    status: "status from a plugin of Muster's",
    resumes: true,
    names: false,
    install: "npm install -g opencode-ai",
  },
  {
    id: "copilot",
    status: "status from its hooks",
    resumes: true,
    names: false,
    install: "npm install -g @github/copilot",
  },
];

/** Every agent by id, Muster's own then the person's. */
export function agentIds(added: readonly AddedAgent[]): string[] {
  return [...OWN_AGENTS.map((agent) => agent.id), ...added.map((agent) => agent.name)];
}

/** The agents that can name chats, by id and label: each session's own first (`App::namers`). */
export function namers(
  added: readonly AddedAgent[],
  label: (id: string) => string,
): { id: string | null; label: string }[] {
  return [
    { id: null, label: "Each session's own agent" },
    ...OWN_AGENTS.filter((agent) => agent.names).map((agent) => ({
      id: agent.id,
      label: label(agent.id),
    })),
    ...added.filter((agent) => agent.asks).map((agent) => ({ id: agent.name, label: agent.label })),
  ];
}
