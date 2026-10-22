import { describe, expect, it } from "vitest";
import type { Session } from "../ipc/generated/Session";
import type { SessionStatus } from "../ipc/generated/SessionStatus";
import { isAgent, secondAgentWarning } from "./secondAgent";

const HERE = "01HZZZZZZZZZZZZZZZZZZZZZZ1";

function session(id: string, harness: string, title: string, status: SessionStatus, at: number) {
  return {
    id,
    workspace: HERE,
    harness,
    title,
    status,
    unseen: false,
    resume_id: null,
    created_at: at,
    ended_at: null,
    status_since: at,
    activity: null,
    spend: null,
    tidy: null,
    beside: null,
    started_by: null,
  } satisfies Session;
}

const byId = (...sessions: Session[]) => Object.fromEntries(sessions.map((one) => [one.id, one]));

describe("another agent where one works (Model::second_agent_warning)", () => {
  it("tells agents from shells, panels and the checks", () => {
    expect(["claude", "my-agent", "plugin:", "plugin:a/b/c"].map(isAgent)).toEqual([
      true,
      true,
      true,
      true,
    ]);
    expect(["shell", "checks", "plugin:zoetrope/graph"].map(isAgent)).toEqual([
      false,
      false,
      false,
    ]);
  });

  it("is said only where an agent still runs, in the terminal UI's words", () => {
    const quiet = byId(
      session("a", "claude", "Ended", { state: "exited", success: true }, 1),
      session("b", "shell", "zsh", { state: "running" }, 2),
      session("c", "checks", "Checks", { state: "running" }, 3),
    );
    expect(secondAgentWarning(quiet, HERE)).toBeNull();
    const one = { ...quiet, d: session("d", "claude", "Fix the login", { state: "finished" }, 4) };
    expect(secondAgentWarning(one, HERE)).toBe(
      "Fix the login already works here: two agents in one place change the same files. A new worktree keeps their work apart.",
    );
    const two = { ...one, e: session("e", "codex", "Tidy", { state: "fresh" }, 5) };
    expect(secondAgentWarning(two, HERE)).toMatch(/^2 agents already work here:/);
    expect(secondAgentWarning(two, "elsewhere")).toBeNull();
  });
});
