import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import { PutBack } from "./PutBack";

const workspace = ulid(11);
beforeEach(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

function setup(number = 2, configure?: (mock: MockBridge) => void) {
  const bridge = new MockBridge(workingDay());
  // The fixture starts after the agent and its shell have stopped; no real daemon.
  for (const session of bridge.world.sessions)
    if (session.workspace === workspace) session.status = { state: "finished" };
  configure?.(bridge);
  const onClose = vi.fn();
  const onDone = vi.fn();
  const result = renderWithApp(
    <PutBack
      workspace={workspace}
      number={number}
      timeline={bridge.world.timelines[workspace] ?? { entries: [] }}
      onClose={onClose}
      onDone={onDone}
    />,
    bridge,
  );
  return { ...result, onClose, onDone };
}

describe("PRD-14 putting a turn back", () => {
  it.each([false, true])(
    "plans first, explains later entries and memory, then restores (branch=%s)",
    async (branch) => {
      const { bridge, onDone } = setup();
      const dialog = await screen.findByRole("dialog");
      await screen.findByRole("button", { name: "Put back the files" });
      expect(bridge.sent).toEqual([{ UndoPlan: { workspace, number: 2 } }]);
      expect(dialog).toHaveTextContent("What 2 later entries changed goes too.");
      expect(dialog).toHaveTextContent("Codex still remembers that turn");
      expect(dialog).toHaveTextContent("before its 1 commit since");
      await userEvent.keyboard(branch ? "b" : "{Enter}");
      await waitFor(() => expect(onDone).toHaveBeenCalledWith(5));
      expect(bridge.sent).toContainEqual({ Undo: { workspace, number: 2, move_branch: branch } });
      const saved = bridge.world.beforeUndo[`${workspace}:5`];
      expect(saved?.uncommitted).toHaveLength(3);
      expect(await bridge.command({ Undo: { workspace, number: 5, move_branch: false } })).toEqual({
        Undone: { entry: 6, moved_branch: false },
      });
      expect(bridge.world.changes[workspace]?.uncommitted).toHaveLength(3);
    },
  );
  it("a files-only plan has no branch action; cancelling sends no Undo", async () => {
    const { bridge, onClose } = setup(4);
    await screen.findByRole("button", { name: "Put back the files" });
    expect(
      screen.queryByRole("button", { name: "Put back with the branch" }),
    ).not.toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
    expect(bridge.sent).toEqual([{ UndoPlan: { workspace, number: 4 } }]);
  });
  it.each([
    "its commits since are pushed, and moving it back would need a force push",
    "no branch is checked out",
    "1234567 is gone from git",
    "it has moved another way since, and its commits since are pushed",
  ])("says why the branch stays: %s", async (refusal) => {
    setup(2, (mock) => {
      const branch = mock.world.undoPlans[`${workspace}:2`]?.branch;
      if (branch) branch.refused = refusal;
    });
    await screen.findByRole("button", { name: "Put back the files" });
    expect(screen.getByRole("dialog")).toHaveTextContent(`stays where it is: ${refusal}`);
    expect(
      screen.queryByRole("button", { name: "Put back with the branch" }),
    ).not.toBeInTheDocument();
  });
  it("describes moving forward again", async () => {
    setup(2, (mock) => {
      const branch = mock.world.undoPlans[`${workspace}:2`]?.branch;
      if (branch) branch.forward = true;
    });
    expect(await screen.findByText(/forward to 1234567, over 1 commit/)).toBeVisible();
  });
  it("an agent at work is refused only on confirm, using the daemon's words", async () => {
    const { bridge, onDone } = setup(2, (mock) => {
      const agent = mock.world.sessions.find((session) => session.id === ulid(101));
      if (agent) agent.status = { state: "needs_you", reason: "permission" };
    });
    await screen.findByRole("button", { name: "Put back the files" });
    await userEvent.keyboard("y");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Sketch the Tauri shell is at work there: put the files back once it has stopped",
    );
    expect(onDone).not.toHaveBeenCalled();
    expect(bridge.world.timelines[workspace]?.entries).toHaveLength(4);
  });
  it("shows files changed since and permits a retry after the daemon can restore them", async () => {
    const { bridge, onDone } = setup(2, (mock) => {
      mock.errors.Undo = {
        code: "conflict",
        message: "the files were not put back: Entry 'src/link.rs' not uptodate. Cannot merge.",
      };
    });
    await screen.findByRole("button", { name: "Put back the files" });
    await userEvent.keyboard("y");
    expect(await screen.findByRole("alert")).toHaveTextContent("not uptodate. Cannot merge.");
    expect(onDone).not.toHaveBeenCalled();
    delete bridge.errors.Undo;
    await userEvent.keyboard("y");
    await waitFor(() => expect(onDone).toHaveBeenCalledWith(5));
  });
  it("shows a delayed plan and failed plan, then asks again", async () => {
    const { bridge } = setup(2, (mock) => mock.held.add("UndoPlan"));
    expect(screen.getByRole("status")).toHaveTextContent(
      "Asking what putting the files back would do…",
    );
    await act(async () => {
      bridge.errors.UndoPlan = {
        code: "not_found",
        message: "there is no entry 2 in this workspace's timeline",
      };
      bridge.release("UndoPlan");
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("there is no entry 2");
    delete bridge.errors.UndoPlan;
    await userEvent.click(screen.getByRole("button", { name: "Ask again" }));
    expect(await screen.findByRole("button", { name: "Put back the files" })).toHaveFocus();
  });
  it("sends one Undo while pending, keeps the dialog open and restores keys on unmount", async () => {
    const { bridge, app, onDone, unmount } = setup(2, (mock) => mock.held.add("Undo"));
    const dialog = await screen.findByRole("dialog");
    await screen.findByRole("button", { name: "Put back the files" });
    await userEvent.keyboard("yyy");
    expect(within(dialog).getByRole("status")).toHaveTextContent("Putting the files back…");
    expect(
      bridge.sent.filter((command) => typeof command !== "string" && "Undo" in command),
    ).toHaveLength(1);
    expect(app.ui.getState().keysIn).toBe("dialog");
    unmount();
    await act(async () => bridge.release("Undo"));
    expect(onDone).not.toHaveBeenCalled();
    expect(app.ui.getState().keysIn).toBe("list");
  });
});
