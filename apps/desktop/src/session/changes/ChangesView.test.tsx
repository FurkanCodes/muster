import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installKeys } from "../../app/keys";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import ChangesView from "./ChangesView";
import { clashWords, entryLabel } from "./labels";

const workspace = ulid(11);
let removeKeys: (() => void) | undefined;
beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.scrollTo = vi.fn();
});
afterEach(() => {
  removeKeys?.();
  vi.unstubAllGlobals();
});

function setup(bridge = new MockBridge(workingDay())) {
  const result = renderWithApp(<ChangesView workspace={workspace} />, bridge);
  removeKeys = installKeys(result.app);
  return result;
}

describe("ARC-24 changes", () => {
  it("shows file status, paths and stats; click and Enter request the daemon's file diff", async () => {
    const { bridge } = setup();
    const file = await screen.findByRole("option", { name: "modified: src/link.rs" });
    expect(within(file).getByText("+118")).toBeVisible();
    await userEvent.click(file);
    expect(await screen.findByRole("region", { name: "Diff" })).toHaveTextContent(
      "use muster_client::{Connection, Received}",
    );
    expect(bridge.sent).toContainEqual({
      FileDiff: { workspace, part: "uncommitted", path: "src/link.rs" },
    });
    await userEvent.keyboard("hj{Enter}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        FileDiff: { workspace, part: "uncommitted", path: "src/main.rs" },
      }),
    );
  });
  it("switches through branch commits and all timeline variants with keys", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.timelines[workspace]?.entries.push(
      {
        number: 5,
        kind: { kind: "undo", of: 2 },
        started: 0,
        ended: 0,
        summary: null,
        head_before: null,
        head_after: null,
      },
      {
        number: 6,
        kind: { kind: "moved", onto: "main" },
        started: 0,
        ended: 0,
        summary: null,
        head_before: null,
        head_after: null,
      },
    );
    setup(bridge);
    await screen.findByRole("option", { name: /modified: src\/link/ });
    await userEvent.keyboard("{Tab}");
    expect(await screen.findByText("Add the desktop crate", { exact: false })).toBeVisible();
    await userEvent.keyboard("{Tab}");
    expect(await screen.findByRole("option", { name: /Codex · turn 3/ })).toBeVisible();
    expect(screen.getByText("Edits between turns")).toBeVisible();
    expect(screen.getByText("Put back as before #2")).toBeVisible();
    expect(screen.getByText("Moved onto main")).toBeVisible();
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({ EntryDiff: { workspace, number: 4 } }),
    );
    await userEvent.keyboard("c");
  });
  it.each(["WorkspaceChanges", "Timeline"])(
    "shows %s loading, error, retry and empty states",
    async (command) => {
      const bridge = new MockBridge(workingDay());
      bridge.held.add(command);
      const { app } = setup(bridge);
      if (command === "Timeline")
        await userEvent.click(screen.getByRole("button", { name: /^Turns/ }));
      expect(
        await screen.findByText(command === "Timeline" ? "Reading the turns…" : "Asking git…"),
      ).toBeVisible();
      await act(async () => {
        bridge.errors[command] = {
          code: "internal",
          message: "Git could not read this workspace.",
        };
        bridge.release(command);
      });
      expect(await screen.findByRole("alert")).toHaveTextContent("Git could not read");
      delete bridge.errors[command];
      bridge.replies[command] =
        command === "Timeline"
          ? { Timeline: { entries: [] } }
          : {
              Changes: { base: "main", branch: [], uncommitted: [], commits: [], more_commits: 0 },
            };
      await userEvent.click(screen.getByRole("button", { name: "Ask again" }));
      expect(
        await screen.findByText(
          command === "Timeline"
            ? "None kept yet: each agent's turn here is, from now on."
            : "Nothing is waiting to be committed.",
        ),
      ).toBeVisible();
      act(() => app.actions.showPane("changes"));
      await userEvent.keyboard("{Escape}");
      expect(app.ui.getState().pane).toBe("terminal");
    },
  );
  it("refreshes on summaries and timeline changes, defers while hidden, ignores unrelated state", async () => {
    const { app, bridge } = setup();
    await screen.findByRole("option", { name: /modified: src\/link/ });
    const count = () =>
      bridge.sent.filter((command) => typeof command !== "string" && "WorkspaceChanges" in command)
        .length;
    const before = count();
    act(() => bridge.updateWorkspace(ulid(12), { timeline: 20 }));
    expect(count()).toBe(before);
    act(() => app.actions.setVisible(false));
    act(() =>
      bridge.updateWorkspace(workspace, {
        timeline: 8,
        changes: { added: 1, removed: 0, files: 1 },
      }),
    );
    expect(count()).toBe(before);
    act(() => app.actions.setVisible(true));
    await waitFor(() => expect(count()).toBe(before + 1));
    act(() => bridge.updateWorkspace(workspace, { timeline: 9 }));
    await waitFor(() => expect(count()).toBe(before + 2));
  });
  it("shows binary, empty, cut and failed diffs in words", async () => {
    const { bridge } = setup();
    bridge.replies.FileDiff = {
      FileDiff: { text: "Binary files a/image.png and b/image.png differ", cut: false },
    };
    await userEvent.click(await screen.findByRole("option", { name: /modified: src\/link/ }));
    expect(await screen.findByText(/binary —/)).toBeVisible();
    bridge.replies.FileDiff = { FileDiff: { text: "", cut: true } };
    await userEvent.keyboard("r");
    expect(await screen.findByText("Git shows no difference in its text.")).toBeVisible();
    expect(screen.getByText(/cut here:/)).toBeVisible();
    bridge.errors.FileDiff = {
      code: "not_found",
      message: "src/link.rs is not among the files changed there",
    };
    await userEvent.keyboard("r");
    expect(await screen.findByRole("alert")).toHaveTextContent("not among the files");
  });
  it("ARC-25 mounts fewer than 100 rows for 5,000 lines and renders the last lines on scroll", async () => {
    const { bridge, container } = setup();
    bridge.replies.FileDiff = {
      FileDiff: {
        text: `@@ -0,0 +1,5000 @@\n${Array.from({ length: 5000 }, (_, n) => `+line ${n}`).join("\n")}`,
        cut: false,
      },
    };
    await userEvent.click(await screen.findByRole("option", { name: /modified: src\/link/ }));
    const diff = await screen.findByRole("region", { name: "Diff" });
    expect(container.querySelectorAll(".mu-dl").length).toBeLessThan(100);
    fireEvent.scroll(diff, { target: { scrollTop: 89900 } });
    expect(within(diff).getByText("line 4999")).toBeVisible();
    expect(container.querySelectorAll(".mu-dl").length).toBeLessThan(100);
  });
  it.each([false, true])(
    "shows the TUI's clash wording (conflict=%s) and opens the other workspace",
    async (conflict) => {
      const { app, bridge } = setup();
      await screen.findByText("Overlap");
      act(() =>
        bridge.updateWorkspace(workspace, {
          clashes: [
            {
              with: ulid(12),
              files: 5,
              conflicts: conflict ? 4 : 0,
              shown: [{ path: "src/a.rs", conflict }],
            },
          ],
        }),
      );
      expect(screen.getByText(conflict ? "Conflict" : "Overlap")).toBeVisible();
      expect(screen.getByRole("status")).toHaveTextContent(
        conflict
          ? "would conflict in src/a.rs and 3 more"
          : "changes them too: src/a.rs and 4 more",
      );
      await userEvent.click(screen.getByRole("button", { name: "See workspace" }));
      expect(app.ui.getState().selected.all).toBe(ulid(103));
    },
  );
});

it("ARC-24 says timeline labels and clash paths as the TUI does", () => {
  const timeline = workingDay().timelines[workspace];
  if (!timeline?.entries[0]) throw new Error("missing fixture");
  expect(entryLabel(timeline, timeline.entries[0])).toBe("Codex · turn 3");
  expect(
    clashWords({
      with: workspace,
      files: 2,
      conflicts: 1,
      shown: [
        { path: "two\nlines", conflict: true },
        { path: "README", conflict: false },
      ],
    }),
  ).toBe("would conflict in two lines");
});
