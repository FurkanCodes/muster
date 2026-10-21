// Status marks, agent logos, counts, pull requests, diff stats, branches
// (design system: Status).

import type { PullRequestStatus } from "../ipc/generated/PullRequestStatus";
import { agentOf } from "../lib/agents";
import { removed } from "../lib/format";
import type { StatusKind } from "../lib/status";
import { Spinner } from "./controls";

const LABELS: Record<StatusKind, string> = {
  working: "working",
  waiting: "needs you",
  done: "finished",
  error: "exited with an error",
  ended: "ended",
  fresh: "starting",
};

const MARKS: Record<Exclude<StatusKind, "working">, string> = {
  waiting: "▲",
  done: "✓",
  error: "✗",
  ended: "○",
  fresh: "○",
};

export function StatusMark({ kind }: { kind: StatusKind }) {
  const tone = kind === "fresh" ? "ended" : kind;
  return (
    <span className={`mu-mark mu-mark--${tone}`} role="img" aria-label={LABELS[kind]}>
      {kind === "working" ? <Spinner /> : MARKS[kind]}
    </span>
  );
}

/** The not-seen dot. */
export function NewsDot() {
  return <span className="mu-dot" role="img" aria-label="not seen" />;
}

export function AgentLogo({
  harness,
  size = "md",
}: {
  harness: string;
  size?: "sm" | "md" | "lg";
}) {
  const agent = agentOf(harness);
  const sizeClass = size === "md" ? "" : ` mu-logo--${size}`;
  if (!agent.logo) {
    return (
      <span
        className={`mu-logo mu-logo--mark mu-logo--${agent.tone}${sizeClass}`}
        role="img"
        aria-label={agent.name}
      >
        {agent.mark}
      </span>
    );
  }
  return (
    <span className={`mu-logo${sizeClass}`}>
      <img src={agent.logo} alt={agent.name} draggable={false} />
    </span>
  );
}

export function Badge({
  count,
  tone,
  label,
}: {
  count: number;
  tone?: "waiting" | "news";
  label: string;
}) {
  if (count <= 0) return null;
  return (
    <span className={`mu-badge ${tone ? `mu-badge--${tone}` : ""}`} role="img" aria-label={label}>
      {count > 99 ? "99+" : count}
    </span>
  );
}

/** The one word that says most of how a pull request stands, and its tone. */
export function pullRequestWord(pr: PullRequestStatus): [string, string] {
  if (pr.state === "open" && pr.review === "approved") return ["approved", "mu-pr-approved"];
  if (pr.state === "open" && pr.review === "changes_requested") return ["changes", "mu-pr-changes"];
  if (pr.state === "open" && pr.review === "required") return ["review", "mu-pr-review"];
  if (pr.state === "merged") return ["merged", "mu-pr-merged"];
  if (pr.state === "closed") return ["closed", "mu-pr-closed"];
  return [pr.state, ""];
}

const CHECKS = {
  passing: ["✓", "mu-check-passed", "checks passed"],
  failing: ["✗", "mu-check-failed", "checks failed"],
  pending: ["◌", "mu-check-running", "checks running"],
} as const;

/** `#88 approved ✓`: a pull request as its git host last said. */
export function PullRequest({ pr, words = true }: { pr: PullRequestStatus; words?: boolean }) {
  const [word, tone] = pullRequestWord(pr);
  const checks = pr.checks ? CHECKS[pr.checks] : null;
  return (
    <span className="mu-pr" title={`#${pr.number}, ${word}`}>
      #{pr.number}
      {words ? <span className={tone}>{word}</span> : null}
      {checks ? (
        <span className={checks[1]} role="img" aria-label={checks[2]}>
          {checks[0]}
        </span>
      ) : null}
    </span>
  );
}

export function DiffStat({
  added,
  removed: gone,
  files,
}: {
  added: number;
  removed: number;
  files?: number;
}) {
  if (added === 0 && gone === 0) {
    return (
      <span className="mu-diff">
        <span className="mu-quiet">no changes</span>
      </span>
    );
  }
  return (
    <span className="mu-diff">
      {added > 0 ? <span className="mu-add">+{added}</span> : null}
      {gone > 0 ? <span className="mu-del">{removed(gone)}</span> : null}
      {files === undefined ? null : (
        <span className="mu-quiet">
          in {files} {files === 1 ? "file" : "files"}
        </span>
      )}
    </span>
  );
}

/** `⎇ branch in project`, the project's name in its colour. */
export function Branch({
  branch,
  project,
  colour = 1,
}: {
  branch: string;
  project?: string;
  colour?: number;
}) {
  return (
    <span className="mu-cut">
      <span className="mu-branch">⎇ {branch}</span>
      {project ? (
        <>
          {" "}
          <span className="mu-where">
            in <span className={`mu-p${colour}`}>{project}</span>
          </span>
        </>
      ) : null}
    </span>
  );
}
