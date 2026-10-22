// Status marks, agent logos, counts, pull requests, diff stats, branches
// (design system: Status).

import type { ReactNode } from "react";
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

/** A status as a shape the stylesheet draws (a dot, a ring), the spinner while it works. */
export function StatusMark({ kind }: { kind: StatusKind }) {
  const tone = kind === "fresh" ? "ended" : kind;
  return (
    <span className={`mu-mark mu-mark--${tone}`} role="img" aria-label={LABELS[kind]}>
      {kind === "working" ? <Spinner /> : null}
    </span>
  );
}

/** A project's colour, a small round mark beside its name (`colour` its place in the list, 1 to 6). */
export function Swatch({ colour }: { colour: number }) {
  return <span className={`mu-swatch mu-p${colour}`} aria-hidden="true" />;
}

/** A mark's room with no mark in it, so the names after it line up with those after a mark. */
export function MarkSpace() {
  return <span className="mu-mark" aria-hidden="true" />;
}

/** A branch's name as the design system says one: after ⎇, in the quiet face it is set in. */
export function BranchName({
  title,
  className,
  children,
}: {
  title?: string;
  /** A text utility: `mu-cut` where it is cut to fit. */
  className?: string;
  children: ReactNode;
}) {
  return (
    <span className={className ? `mu-branch ${className}` : "mu-branch"} title={title}>
      ⎇ {children}
    </span>
  );
}

/** Where work meets other work: an overlap (Git could merge it) or a conflict. */
export function Clash({ kind }: { kind: "overlap" | "conflict" }) {
  return <span className={`mu-clash-${kind}`}>{kind}</span>;
}

/** Something looked for and found ("found", "on PATH"), in the finished colour. */
export function Found({ children }: { children: ReactNode }) {
  return <span className="mu-found">{children}</span>;
}

/** Something looked for and not found, in the waiting colour. */
export function Missing({ children }: { children: ReactNode }) {
  return <span className="mu-missing">{children}</span>;
}

/** A version, as a program writes it: quiet, in mono. */
export function Version({ children }: { children: ReactNode }) {
  return <span className="mu-version">{children}</span>;
}

/** What went wrong, said in the waiting colour where it happened; `meta` under a field or a list. */
export function ErrorText({
  size = "body",
  children,
}: {
  size?: "body" | "meta";
  children: ReactNode;
}) {
  return (
    <p className={`mu-error-text mu-error-text--${size}`} role="alert">
      {children}
    </p>
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
    return <AgentMark mark={agent.mark} label={agent.name} tone={agent.tone} size={size} />;
  }
  return (
    <span className={`mu-logo${sizeClass}`}>
      <img src={agent.logo} alt={agent.name} draggable={false} />
    </span>
  );
}

/** An agent's mark where it has no logo of its own (one the person added): a character in a square. */
export function AgentMark({
  mark,
  label,
  tone,
  size = "md",
}: {
  mark: string;
  label: string;
  /** Its own colour, where it is one of Muster's agents. */
  tone?: string;
  size?: "sm" | "md" | "lg";
}) {
  const classes = [
    "mu-logo",
    "mu-logo--mark",
    tone ? `mu-logo--${tone}` : "",
    size === "md" ? "" : `mu-logo--${size}`,
  ];
  return (
    <span className={classes.filter(Boolean).join(" ")} role="img" aria-label={label}>
      {mark}
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
