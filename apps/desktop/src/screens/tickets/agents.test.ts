import { describe, expect, it } from "vitest";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import {
  agentsOn,
  agentToOpen,
  issueOfBranch,
  ticketBranch,
  ticketPullRequest,
  worktreesFor,
} from "./agents";

describe("ARC-24 ticket branches follow muster_core::tickets", () => {
  it.each([
    ["muster/issue-42", "42"],
    ["muster/issue-42-fix-the-login-2", "42"],
    ["issue-PROJ-7-fix-it-2", "PROJ-7"],
    ["issue-A_B2-009-thing", "A_B2-009"],
    ["issue-007-title", "7"],
    ["issue-18446744073709551615", "18446744073709551615"],
    ["issue-18446744073709551616", null],
    ["muster/issue-fix-12", null],
    ["muster/issue-x-fix", null],
    ["issue-", null],
    ["feature/issue-42", null],
    ["tissue-42", null],
    ["detached at 1a2b3c", null],
    ["issue-1ABC-7", null],
  ])("%s belongs to %s", (branch, key) => expect(issueOfBranch(branch)).toBe(key));
  it("keeps whole title words within the core's slug limit", () => {
    expect(ticketBranch("42", "Fix the login redirect!", false)).toBe(
      "issue-42-fix-the-login-redirect",
    );
    expect(ticketBranch("PROJ-7", "word ".repeat(30))).toBe(
      `muster/issue-PROJ-7-${Array(8).fill("word").join("-")}`,
    );
    expect(ticketBranch("42", "")).toBe("muster/issue-42");
  });
  it("joins shared projects' worktrees made for it and excludes shells and plugin panels", () => {
    const world = workingDay();
    const tree = world.workspaces.find((w) => w.id === ulid(11));
    if (!tree) throw new Error("missing worktree fixture");
    // Made for #214, whatever its branch is called now.
    tree.ticket = "214";
    tree.kind.branch = "fix/desktop-app";
    // A branch of the person's own named like a ticket's is never one.
    const theirs = world.workspaces.find((w) => w.id === ulid(12));
    if (!theirs) throw new Error("missing worktree fixture");
    theirs.kind.branch = "issue-214-mine";
    theirs.ticket = null;
    const trees = worktreesFor(world.workspaces, [ulid(1)], "214");
    expect(trees.map((w) => w.id)).toEqual([ulid(11)]);
    const agents = agentsOn(world.sessions, trees);
    const agent = agents[0];
    if (!agent) throw new Error("missing agent fixture");
    expect(agents.map((s) => s.id)).toEqual([ulid(101)]);
    expect(
      agentToOpen([...agents, { ...agent, id: "ended", created_at: 1e20, ended_at: 1 }])?.id,
    ).toBe(ulid(101));
  });
  it("shows merged before open before draft before closed; newest number breaks ties", () => {
    const base = workingDay().workspaces[0];
    if (!base) throw new Error("missing workspace fixture");
    const prs = ["closed", "draft", "open", "merged"].map((state, i) => ({
      ...base,
      pull_request: { number: i + 1, url: "", state, checks: null, review: null },
    })) as Parameters<typeof ticketPullRequest>[0];
    expect(ticketPullRequest(prs)?.state).toBe("merged");
    expect(ticketPullRequest(prs.slice(0, 3))?.state).toBe("open");
    const open = prs[2]?.pull_request;
    if (!open) throw new Error("missing PR fixture");
    expect(
      ticketPullRequest([{ ...base, pull_request: { ...open, number: 90 } }, ...prs.slice(0, 3)])
        ?.number,
    ).toBe(90);
  });
});
