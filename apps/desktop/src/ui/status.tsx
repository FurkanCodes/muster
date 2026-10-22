// Status marks, agent logos, counts, pull requests, diff stats, branches
// (design system: Status).

import type { PullRequestStatus } from "../ipc/generated/PullRequestStatus";
import { agentOf } from "../lib/agents";
import { removed } from "../lib/format";
import type { StatusKind } from "../lib/status";
import { Spinner } from "./controls";
import { type PartProps, usePart } from "./part";

const LABELS: Record<StatusKind, string> = {
  working: "working",
  waiting: "needs you",
  done: "finished",
  error: "exited with an error",
  ended: "ended",
  fresh: "starting",
};

/** A status as a shape the stylesheet draws (a dot, a ring), the spinner while it works. */
export function StatusMark({ kind, ...props }: PartProps<"span"> & { kind: StatusKind }) {
  const tone = kind === "fresh" ? "ended" : kind;
  return usePart("span", props, {
    className: `mu-mark mu-mark--${tone}`,
    role: "img",
    "aria-label": LABELS[kind],
    children: kind === "working" ? <Spinner /> : null,
  });
}

/** A project's colour, a small round mark beside its name (`colour` its place in the list, 1 to 6). */
export function Swatch({ colour, ...props }: PartProps<"span"> & { colour: number }) {
  return usePart("span", props, { className: `mu-swatch mu-p${colour}`, "aria-hidden": true });
}

/** A mark's room with no mark in it, so the names after it line up with those after a mark. */
export function MarkSpace(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-mark", "aria-hidden": true });
}

/** A branch's name as the design system says one: after ⎇, in the quiet face it is set in. */
export function BranchName({ children, ...props }: PartProps<"span">) {
  return usePart("span", props, {
    className: "mu-branch",
    children: <>⎇ {children}</>,
  });
}

/** Where work meets other work: an overlap (Git could merge it) or a conflict. */
export function Clash({ kind, ...props }: PartProps<"span"> & { kind: "overlap" | "conflict" }) {
  return usePart("span", props, { className: `mu-clash-${kind}`, children: kind });
}

/** Something looked for and found ("found", "on PATH"), in the finished colour. */
export function Found(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-found" });
}

/** Something looked for and not found, in the waiting colour. */
export function Missing(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-missing" });
}

/** A version, as a program writes it: quiet, in mono. */
export function Version(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-version" });
}

/** What went wrong, said in the waiting colour where it happened; `meta` under a field or a list. */
export function ErrorText({
  size = "body",
  ...props
}: PartProps<"p"> & { size?: "body" | "meta" }) {
  return usePart("p", props, { className: `mu-error-text mu-error-text--${size}`, role: "alert" });
}

/** The not-seen dot. */
export function NewsDot(props: PartProps<"span">) {
  return usePart("span", props, { className: "mu-dot", role: "img", "aria-label": "not seen" });
}

export function AgentLogo({
  harness,
  size = "md",
  ...props
}: PartProps<"span"> & { harness: string; size?: "sm" | "md" | "lg" }) {
  const agent = agentOf(harness);
  const sizeClass = size === "md" ? "" : ` mu-logo--${size}`;
  const logo = usePart("span", props, {
    className: `mu-logo${sizeClass}`,
    children: agent.logo ? <img src={agent.logo} alt={agent.name} draggable={false} /> : null,
  });
  if (!agent.logo) {
    return (
      <AgentMark mark={agent.mark} label={agent.name} tone={agent.tone} size={size} {...props} />
    );
  }
  return logo;
}

/** An agent's mark where it has no logo of its own (one the person added): a character in a square. */
export function AgentMark({
  mark,
  label,
  tone,
  size = "md",
  ...props
}: PartProps<"span"> & {
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
  return usePart("span", props, {
    className: classes.filter(Boolean).join(" "),
    role: "img",
    "aria-label": label,
    children: mark,
  });
}

export function Badge({
  count,
  tone,
  label,
  ...props
}: PartProps<"span"> & { count: number; tone?: "waiting" | "news"; label: string }) {
  const badge = usePart("span", props, {
    className: `mu-badge ${tone ? `mu-badge--${tone}` : ""}`,
    role: "img",
    "aria-label": label,
    children: count > 99 ? "99+" : count,
  });
  return count > 0 ? badge : null;
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
export function PullRequest({
  pr,
  words = true,
  ...props
}: PartProps<"span"> & { pr: PullRequestStatus; words?: boolean }) {
  const [word, tone] = pullRequestWord(pr);
  const checks = pr.checks ? CHECKS[pr.checks] : null;
  return usePart("span", props, {
    className: "mu-pr",
    title: `#${pr.number}, ${word}`,
    children: (
      <>
        #{pr.number}
        {words ? <span className={tone}>{word}</span> : null}
        {checks ? (
          <span className={checks[1]} role="img" aria-label={checks[2]}>
            {checks[0]}
          </span>
        ) : null}
      </>
    ),
  });
}

export function DiffStat({
  added,
  removed: gone,
  files,
  ...props
}: PartProps<"span"> & { added: number; removed: number; files?: number }) {
  return usePart("span", props, {
    className: "mu-diff",
    children:
      added === 0 && gone === 0 ? (
        <span className="mu-quiet">no changes</span>
      ) : (
        <>
          {added > 0 ? <span className="mu-add">+{added}</span> : null}
          {gone > 0 ? <span className="mu-del">{removed(gone)}</span> : null}
          {files === undefined ? null : (
            <span className="mu-quiet">
              in {files} {files === 1 ? "file" : "files"}
            </span>
          )}
        </>
      ),
  });
}

/** `⎇ branch in project`, the project's name in its colour. */
export function Branch({
  branch,
  project,
  colour = 1,
  ...props
}: PartProps<"span"> & { branch: string; project?: string; colour?: number }) {
  return usePart("span", props, {
    className: "mu-cut",
    children: (
      <>
        <span className="mu-branch">⎇ {branch}</span>
        {project ? (
          <>
            {" "}
            <span className="mu-where">
              in <span className={`mu-p${colour}`}>{project}</span>
            </span>
          </>
        ) : null}
      </>
    ),
  });
}
