// Which coding agent a session runs, by its harness id: its name, its logo
// (the Agents assets) and its mark where no logo is drawn.

import claude from "../assets/agents/claude.svg";
import codex from "../assets/agents/codex.svg";
import copilot from "../assets/agents/copilot.svg";
import gemini from "../assets/agents/gemini.svg";
import opencode from "../assets/agents/opencode.svg";
import type { OwnAgent } from "../ipc/generated/OwnAgent";

/** Muster's own coding agents, in the order offered (`muster_core::harness::OwnAgent`). */
export const OWN: readonly OwnAgent[] = ["claude", "codex", "gemini", "opencode", "copilot"];

export interface Agent {
  name: string;
  logo: string | null;
  mark: string;
  /** The `mu-logo--*` modifier for its mark's colour. */
  tone: OwnAgent | "shell" | "added";
}

const KNOWN: Record<OwnAgent | "shell", Agent> = {
  claude: { name: "Claude Code", logo: claude, mark: "✻", tone: "claude" },
  codex: { name: "Codex", logo: codex, mark: "◎", tone: "codex" },
  gemini: { name: "Gemini CLI", logo: gemini, mark: "✦", tone: "gemini" },
  opencode: { name: "OpenCode", logo: opencode, mark: "▣", tone: "opencode" },
  copilot: { name: "Copilot CLI", logo: copilot, mark: "◉", tone: "copilot" },
  shell: { name: "Shell", logo: null, mark: "❯", tone: "shell" },
};

export function agentOf(harness: string): Agent {
  const known = (KNOWN as Record<string, Agent | undefined>)[harness];
  if (known) return known;
  if (harness.startsWith("plugin:")) {
    const name = harness.slice("plugin:".length).split("/")[1] ?? harness;
    return { name, logo: null, mark: "◇", tone: "shell" };
  }
  return { name: harness, logo: null, mark: "•", tone: "added" };
}
