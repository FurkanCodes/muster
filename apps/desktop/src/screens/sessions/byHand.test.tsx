// The stages by hand from the All list, on the selected session's worktree,
// as the terminal UI's keys run them:
// c the checks and v a review, each a session opened with the keys; o a
// draft pull request, said in the status bar; b the pull request in the
// browser; and the TUI's words where none can be (PRD §5 "By hand", ARC-24).

import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../../app/keys";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderList } from "../../test/render";

let uninstall = () => {};
beforeEach(() => localStorage.clear());
afterEach(() => uninstall());

async function shown(bridge = new MockBridge(workingDay())) {
  const rendered = renderList("all", true, bridge);
  uninstall = installKeys(rendered.app);
  act(() => rendered.app.actions.show("all"));
  await screen.findByRole("tree");
  return rendered;
}

const row = (name: string | RegExp) =>
  screen.getByRole("treeitem", {
    name: typeof name === "string" ? (accessible: string) => accessible === name : name,
  });

describe("PRD §5 the stages by hand, from All sessions", () => {
  it("c runs the checks as a session of their own, opened with the keys", async () => {
    const { app, bridge } = await shown();
    await userEvent.click(row(/^Flaky resize test/));
    await userEvent.keyboard("c");
    await waitFor(() => expect(app.ui.getState().keysIn).toBe("terminal"));
    expect(bridge.sent).toContainEqual({
      CheckWorkspace: { workspace: ulid(12), size: { cols: 100, rows: 30 } },
    });
    const checks = app.daemon.store.getState().sessions[app.ui.getState().selected.all ?? ""];
    expect(checks).toMatchObject({ harness: "checks", title: "Checks", workspace: ulid(12) });
    expect(app.ui.getState().terminal).toBe(checks?.id);
  });

  it("v has Claude Code review the change, opened with the keys", async () => {
    const { app, bridge } = await shown();
    await userEvent.click(row(/^Resize settles/));
    await userEvent.keyboard("v");
    await waitFor(() => expect(app.ui.getState().keysIn).toBe("terminal"));
    expect(bridge.sent).toContainEqual({
      ReviewWorkspace: { workspace: ulid(12), size: { cols: 100, rows: 30 } },
    });
    expect(app.daemon.store.getState().sessions[app.ui.getState().terminal ?? ""]).toMatchObject({
      harness: "review",
      title: "Review",
    });
  });

  it("o pushes the branch and opens a draft pull request, says so, and b opens it", async () => {
    const { app, bridge } = await shown();
    await userEvent.click(row(/^Upgrade AGP/));
    await userEvent.keyboard("b");
    expect(app.ui.getState().note).toEqual({
      text: "No pull request from this branch is known; o opens one.",
      error: true,
    });
    // The push and the host take a while: said while they do.
    bridge.hold("OpenPullRequest");
    await userEvent.keyboard("o");
    expect(app.ui.getState().note?.text).toBe(
      "Pushing the branch and opening a draft pull request…",
    );
    bridge.release("OpenPullRequest");
    await waitFor(() =>
      expect(app.ui.getState().note?.text).toBe(
        "Draft pull request #120 is open: https://github.com/o/lamphaus/pull/120",
      ),
    );
    expect(bridge.sent).toContainEqual({ OpenPullRequest: { workspace: ulid(22) } });
    // The session's head shows it, and b opens it.
    expect(screen.getByRole("region", { name: "Upgrade AGP to 9.1" })).toHaveTextContent("#120");
    await userEvent.keyboard("b");
    await waitFor(() =>
      expect(bridge.openedUrls).toEqual(["https://github.com/o/lamphaus/pull/120"]),
    );
  });

  it("says why where none can be: a project's line, the base branch, no checks named, a push refused", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.byHand.checks.delete(ulid(1));
    const { app } = await shown(bridge);
    const said = () => app.ui.getState().note;
    await userEvent.click(row("lamphaus"));
    await userEvent.keyboard("c");
    expect(said()).toEqual({
      text: "Select a session in a worktree to run the checks there.",
      error: true,
    });
    await userEvent.keyboard("v");
    expect(said()?.text).toBe("Select a session in a worktree to review its change.");
    await userEvent.keyboard("o");
    expect(said()?.text).toBe("Select a session in a worktree to open a pull request from it.");

    // The project's own checkout is on the branch a pull request merges into.
    await userEvent.click(row(/^Split view\.rs/));
    await userEvent.keyboard("o");
    await waitFor(() =>
      expect(said()).toEqual({
        text: "main is the branch a pull request would merge into",
        error: true,
      }),
    );
    await userEvent.keyboard("c");
    await waitFor(() =>
      expect(said()?.text).toBe(
        'the project names no checks: add `[checks] commands = ["…"]` to muster.toml',
      ),
    );
    expect(app.ui.getState().keysIn).toBe("list");

    bridge.failNext(
      "OpenPullRequest",
      "could not push fix/resize-under-load: no remote",
      "bad_request",
    );
    await userEvent.click(row(/^Flaky resize test/));
    // One is open already: found, not opened again.
    await userEvent.keyboard("o");
    await waitFor(() =>
      expect(said()?.text).toBe("could not push fix/resize-under-load: no remote"),
    );
  });

  it("answers the person's keys, and offers them in the session's more menu", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.preferences.keys = { checks: ["C"], open_pull_request: ["O"] };
    const { app } = await shown(bridge);
    await userEvent.click(row(/^Flaky resize test/));
    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    expect(screen.getByRole("menuitem", { name: /^Run the checks/ })).toHaveTextContent("⇧C");
    expect(screen.getByRole("menuitem", { name: /^Open a draft pull request/ })).toHaveTextContent(
      "⇧O",
    );
    await userEvent.keyboard("{Escape}");
    await userEvent.keyboard("c");
    expect(bridge.sent.some((sent) => typeof sent === "object" && "CheckWorkspace" in sent)).toBe(
      false,
    );
    await userEvent.keyboard("C");
    await waitFor(() => expect(app.ui.getState().keysIn).toBe("terminal"));
  });
});
