import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Command } from "../../ipc/generated/Command";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import { beforeWhat, branchWords, PutBack } from "./PutBack";

const workspace = ulid(11);
beforeEach(() => {
  // jsdom has no modal dialogs; WebKit's own is tested end to end (e2e/).
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

function named(name: string) {
  return (command: Command) => typeof command !== "string" && name in command;
}

function setup(number = 2, configure?: (mock: MockBridge) => void) {
  const bridge = new MockBridge(workingDay());
  // The agent there has finished its turn, so nothing works in the workspace.
  for (const session of bridge.world.sessions)
    if (session.workspace === workspace) session.status = { state: "finished" };
  configure?.(bridge);
  const onClose = vi.fn();
  const onDone = vi.fn();
  const result = renderWithApp(
    <PutBack
      workspace={workspace}
      number={number}
      timeline={bridge.world.timelines?.[workspace] ?? { entries: [] }}
      onClose={onClose}
      onDone={onDone}
    />,
    bridge,
  );
  return { ...result, onClose, onDone };
}

describe("PRD-14 putting a turn back", () => {
  it("asks the plan before it asks the person, and says what goes back as the board does", async () => {
    const { bridge } = setup(2, (mock) => mock.hold("UndoPlan"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(bridge.sent).toEqual([{ UndoPlan: { workspace, number: 2 } }]);
    await act(async () => bridge.release("UndoPlan"));
    const dialog = await screen.findByRole("dialog", {
      name: "Put the files back as before Codex's turn 2?",
    });
    expect(dialog).toHaveTextContent("The worktree's files go back as they were 28m ago.");
    expect(dialog).toHaveTextContent("What is there now is kept first, so this can be put back");
    expect(dialog).toHaveTextContent(
      "Goes with itEdits between turns, 5m agoCodex, turn 3, 1m ago",
    );
    expect(dialog).toHaveTextContent(
      "Put back with the branch also moves ⎇ feature/desktop-app back to 1234567, before its 1 commit since.",
    );
    expect(dialog).toHaveTextContent("CodexStill remembers that turn: tell it what you put back.");
    // The files only is the acting answer, with the focus: the branch is chosen (PRD-14).
    const files = screen.getByRole("button", { name: "Put back the files" });
    expect(files).toHaveFocus();
    expect(files).toHaveClass("mu-btn--primary");
    expect(screen.getByRole("button", { name: "Put back with the branch" })).toHaveClass(
      "mu-btn--secondary",
    );
  });

  it.each([
    ["y", false],
    ["b", true],
    ["{Enter}", false],
  ])(
    "%s sends Undo (branch=%s), says it back and hands over the new entry",
    async (key, branch) => {
      const { bridge, app, onDone } = setup();
      await screen.findByRole("dialog");
      expect(app.ui.getState().keysIn).toBe("dialog");
      await userEvent.keyboard(key);
      await waitFor(() => expect(onDone).toHaveBeenCalledWith(5));
      expect(bridge.sent).toContainEqual({ Undo: { workspace, number: 2, move_branch: branch } });
      expect(app.ui.getState().note).toEqual({
        text: `The files are as they were before #2${branch ? ", and the branch with them" : ""}. z on #5 brings them back.`,
        error: false,
      });
      expect(bridge.world.timelines?.[workspace]?.entries[0]?.kind).toEqual({
        kind: "undo",
        of: 2,
      });
    },
  );

  it("a plan without the branch offers the files only, acting on ⏎; n and ⎋ keep it", async () => {
    const { bridge, onClose, onDone } = setup(4);
    const files = await screen.findByRole("button", { name: "Put back the files" });
    expect(files).toHaveFocus();
    expect(files).toHaveClass("mu-btn--primary");
    expect(screen.queryByRole("button", { name: "Put back with the branch" })).toBeNull();
    expect(screen.getByRole("dialog")).not.toHaveTextContent("Goes with it");
    await userEvent.keyboard("n");
    await userEvent.click(screen.getByRole("button", { name: "Keep it" }));
    expect(onClose).toHaveBeenCalledTimes(2);
    expect(onDone).not.toHaveBeenCalled();
    expect(bridge.sent.filter(named("Undo"))).toEqual([]);
  });

  it.each([
    "its commits since are pushed, and moving it back would need a force push",
    "no branch is checked out",
    "1234567 is gone from git",
    "it has moved another way since, and its commits since are pushed",
  ])("says why the branch stays, and b does nothing: %s", async (refusal) => {
    const { bridge } = setup(2, (mock) => {
      const branch = mock.world.undoPlans?.[`${workspace}:2`]?.branch;
      if (branch) branch.refused = refusal;
    });
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent(`⎇ feature/desktop-app stays where it is: ${refusal}.`);
    expect(screen.queryByRole("button", { name: "Put back with the branch" })).toBeNull();
    await userEvent.keyboard("b");
    expect(bridge.sent.filter(named("Undo"))).toEqual([]);
  });

  it("an agent at work there: the daemon's refusal goes to the status bar, as in the TUI", async () => {
    const { bridge, app, onClose, onDone } = setup(2, (mock) => {
      const agent = mock.world.sessions.find((session) => session.id === ulid(101));
      if (agent) agent.status = { state: "needs_you", reason: "permission" };
    });
    await screen.findByRole("dialog");
    await userEvent.keyboard("y");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(app.ui.getState().note).toEqual({
      text: "Sketch the Tauri shell is at work there: put the files back once it has stopped",
      error: true,
    });
    expect(onDone).not.toHaveBeenCalled();
    expect(bridge.world.timelines?.[workspace]?.entries).toHaveLength(4);
  });

  it("files changed since: git's refusal goes to the status bar", async () => {
    const { app, onClose } = setup(2, (mock) => {
      mock.failNext(
        "Undo",
        "the files were not put back: error: Entry 'src/link.rs' not uptodate. Cannot merge.",
        "conflict",
      );
    });
    await screen.findByRole("dialog");
    await userEvent.keyboard("y");
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(app.ui.getState().note?.text).toBe(
      "the files were not put back: error: Entry 'src/link.rs' not uptodate. Cannot merge.",
    );
  });

  it("a refused plan opens no dialog and says why", async () => {
    const { app, onClose } = setup(9);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(app.ui.getState().note).toEqual({
      text: "there is no entry 9 in this workspace's timeline",
      error: true,
    });
  });

  it("is busy while the daemon puts the files back: one Undo, no second press", async () => {
    const { bridge, app, onDone, unmount } = setup(2, (mock) => mock.hold("Undo"));
    await screen.findByRole("dialog");
    await userEvent.keyboard("yyb");
    const busy = screen.getByRole("button", { name: "Putting back…" });
    expect(busy).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Keep it" })).toBeDisabled();
    expect(bridge.sent.filter(named("Undo"))).toHaveLength(1);
    unmount();
    await act(async () => bridge.release("Undo"));
    expect(onDone).not.toHaveBeenCalled();
    expect(app.ui.getState().keysIn).toBe("list");
  });
});

describe("PRD-14 the dialog's words", () => {
  const timeline = workingDay().timelines?.[workspace] ?? { entries: [] };
  it("names what the files go back to", () => {
    const at = (number: number) => timeline.entries.find((entry) => entry.number === number);
    expect(beforeWhat(timeline, at(4), 4)).toBe("Codex's turn 3");
    expect(beforeWhat(timeline, at(3), 3)).toBe("the edits between turns");
    expect(beforeWhat(timeline, undefined, 7)).toBe("#7");
  });
  it("moves the branch forward again where it went back", () => {
    expect(
      branchWords({ branch: "main", to: "abc1234", commits: 2, forward: true, refused: null }),
    ).toBe("Put back with the branch also moves ⎇ main forward to abc1234, over 2 commits.");
  });
});
