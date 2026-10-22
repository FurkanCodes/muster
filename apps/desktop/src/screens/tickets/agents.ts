// A small port of muster_core::tickets, until the shared model exposes it.

import type { PullRequestStatus } from "../../ipc/generated/PullRequestStatus";
import type { Session } from "../../ipc/generated/Session";
import type { TicketKey } from "../../ipc/generated/TicketKey";
import type { Workspace } from "../../ipc/generated/Workspace";
import { standsBeside } from "../../state/selectors";

export function issueOfBranch(branch: string): TicketKey | null {
  const rest = branch.startsWith("muster/issue-")
    ? branch.slice(13)
    : branch.startsWith("issue-")
      ? branch.slice(6)
      : null;
  if (rest === null) return null;
  const [first = "", second = ""] = rest.split("-");
  if (/^\d+$/.test(first)) {
    const number = BigInt(first);
    return first.length <= 40 && number <= 18446744073709551615n ? String(number) : null;
  }
  const key = `${first}-${second}`;
  return key.length <= 40 && /^[A-Z][A-Z0-9_]*-\d+$/.test(key) ? key : null;
}

export function shownKey(key: TicketKey): string {
  return /^\d+$/.test(key) ? `#${key}` : key;
}

export function ticketBranch(key: TicketKey, title: string, prefixed = true): string {
  let slug = "";
  for (const word of title
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(Boolean)) {
    if (slug.length + word.length + 1 > 40) break;
    slug += `${slug ? "-" : ""}${word}`;
  }
  return `${prefixed ? "muster/" : ""}issue-${key}${slug ? `-${slug}` : ""}`;
}

/** `Model::worktrees_for`: the worktrees made for ticket `key`, whatever their branches are called now. */
export function worktreesFor(
  workspaces: readonly Workspace[],
  projects: readonly string[],
  key: TicketKey,
): Workspace[] {
  return workspaces.filter(
    (w) => w.kind.kind === "worktree" && projects.includes(w.project) && w.ticket === key,
  );
}

export function agentsOn(sessions: readonly Session[], worktrees: readonly Workspace[]): Session[] {
  const ids = new Set(worktrees.map((w) => w.id));
  return sessions
    .filter((s) => ids.has(s.workspace) && !standsBeside(s))
    .sort((a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id));
}

export function agentToOpen(agents: readonly Session[]): Session | undefined {
  return agents.findLast((s) => s.ended_at === null) ?? agents.at(-1);
}

export function ticketPullRequest(worktrees: readonly Workspace[]): PullRequestStatus | null {
  const rank = { merged: 4, open: 3, draft: 2, closed: 1 };
  let best: PullRequestStatus | null = null;
  for (const { pull_request: pr } of worktrees) {
    if (
      pr &&
      (!best ||
        rank[pr.state] > rank[best.state] ||
        (rank[pr.state] === rank[best.state] && pr.number > best.number))
    )
      best = pr;
  }
  return best;
}
