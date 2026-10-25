// Spaces and terminals in no project in the window (`ARC-33`, PRD §6a):
// everything lives in one space, and the All list shows each under its
// heading, as the model lists it; the list's heading says Spaces, with a new
// terminal and + beside it; a space heading's
// menu adds to it, renames and removes it, which the daemon refuses while
// it holds anything; a project's and a terminal's menu move it to another
// space; a terminal goes under a project where its shell is; and every
// window opens on everything.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../app/keys";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { createUi } from "../state/ui";
import { renderApp, renderList } from "../test/render";

const lamphaus = ulid(2);
let uninstall = () => {};
beforeEach(() => localStorage.clear());
afterEach(() => uninstall());

async function shown(bridge = new MockBridge(workingDay())) {
  const rendered = renderList("all", true, bridge);
  uninstall = installKeys(rendered.app);
  await screen.findByRole("tree");
  return rendered;
}

const row = (name: string | RegExp) =>
  screen.getByRole("treeitem", {
    name: typeof name === "string" ? (accessible: string) => accessible === name : name,
  });

const startTerminal = (bridge: MockBridge, title: string, space: string | null) =>
  bridge.command({
    StartSession: {
      workspace: null,
      harness: "shell",
      title,
      size: { cols: 80, rows: 24 },
      prompt: null,
      started_by: null,
      space,
    },
  });

/** Right-clicks the line named `name`, as the mouse does. */
function rightClick(name: string | RegExp) {
  const line = row(name);
  line.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
}

describe("ARC-33 spaces in the window", () => {
  it("lists each space under its heading, only the space shown, and everything again", async () => {
    const { app, bridge } = await shown();
    await act(() => bridge.command({ CreateSpace: { name: "Home" } }));
    const space = bridge.world.spaces?.[0]?.id ?? "";
    await act(() => bridge.command({ MoveToSpace: { space, member: { Project: lamphaus } } }));
    await waitFor(() => expect(row("Home")).toBeInTheDocument());
    expect(row("muster")).toBeInTheDocument();
    act(() => app.actions.showSpace(space));
    await waitFor(() => expect(screen.queryByRole("treeitem", { name: "muster" })).toBeNull());
    expect(row("lamphaus")).toBeInTheDocument();
    // A space alone shows no heading of its own.
    expect(screen.queryByRole("treeitem", { name: "Home" })).toBeNull();
    act(() => app.actions.showSpace(null));
    await waitFor(() => expect(row("muster")).toBeInTheDocument());
  });

  it("lists a terminal in no space after the spaces, and one in a space under its heading, which folds it away", async () => {
    const { bridge } = await shown();
    // Asked in no space, it roams: listed, and no space made for it.
    await act(() => startTerminal(bridge, "roam", null));
    expect(await screen.findByRole("treeitem", { name: /^Shell: roam/ })).toBeInTheDocument();
    expect(bridge.world.spaces ?? []).toEqual([]);
    await act(() => bridge.command({ CreateSpace: { name: "Home" } }));
    const home = bridge.world.spaces?.[0]?.id ?? null;
    await act(() => startTerminal(bridge, "vm", home));
    const heading = await screen.findByRole("treeitem", { name: "Home" });
    expect(row(/^Shell: vm/)).toBeInTheDocument();
    await userEvent.click(heading);
    await userEvent.keyboard("e");
    await waitFor(() => expect(screen.queryByRole("treeitem", { name: /^Shell: vm/ })).toBeNull());
    await userEvent.keyboard("e");
    expect(await screen.findByRole("treeitem", { name: /^Shell: vm/ })).toBeInTheDocument();
  });

  it("heads the list Spaces, with a terminal and a new space beside it, and no filter", async () => {
    const { bridge } = renderApp();
    const nav = await screen.findByRole("navigation", { name: "Sessions" });
    expect(within(nav).getByText("Spaces")).toBeInTheDocument();
    await act(() => bridge.command({ CreateSpace: { name: "Client" } }));
    await screen.findByRole("treeitem", { name: "Client" });
    // Every space is listed, under its heading: there is no space to choose.
    expect(screen.queryByRole("button", { name: "Spaces" })).toBeNull();
    expect(within(nav).getByRole("button", { name: "Terminal" })).toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: "New space" })).toBeInTheDocument();
  });

  it("starts a terminal in no space from the heading, and makes a space from its +", async () => {
    const bridge = new MockBridge(workingDay());
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    await userEvent.click(screen.getByRole("button", { name: "Terminal" }));
    await waitFor(() =>
      expect(bridge.sent).toContainEqual(
        expect.objectContaining({
          StartSession: expect.objectContaining({ workspace: null, space: null }),
        }),
      ),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "New space" }));
    const named = await screen.findByRole("dialog", { name: "New space" });
    await userEvent.type(within(named).getByLabelText("Name"), "Lab");
    await userEvent.click(within(named).getByRole("button", { name: /Make it/ }));
    expect(await screen.findByRole("treeitem", { name: "Lab" })).toBeInTheDocument();
  });

  it("a space's line starts a terminal in it, and a project's an agent in a new worktree", async () => {
    const bridge = new MockBridge(workingDay());
    await bridge.command({ CreateSpace: { name: "Lab" } });
    const lab = bridge.world.spaces?.[0]?.id ?? "";
    await bridge.command({ MoveToSpace: { space: lab, member: { Project: lamphaus } } });
    renderApp(bridge);
    await screen.findByRole("treeitem", { name: "Lab" });
    await userEvent.click(screen.getByRole("button", { name: "Terminal in Lab" }));
    await waitFor(() =>
      expect(bridge.world.spaces?.[0]?.members).toContainEqual(
        expect.objectContaining({ Session: expect.any(String) }),
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "New in lamphaus" }));
    const menu = await screen.findByRole("menu", { name: "New in lamphaus" });
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Codex" }));
    await waitFor(() =>
      expect(bridge.sent).toContainEqual(
        expect.objectContaining({ CreateWorktree: expect.objectContaining({ project: lamphaus }) }),
      ),
    );
    await waitFor(() =>
      expect(bridge.sent).toContainEqual(
        expect.objectContaining({ StartSession: expect.objectContaining({ harness: "codex" }) }),
      ),
    );
  });

  it("files a terminal in no project under the project its shell is in", async () => {
    const bridge = new MockBridge(workingDay());
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    const reply = await act(() => startTerminal(bridge, "build", null));
    const id =
      typeof reply === "object" && "SessionStarted" in reply ? reply.SessionStarted.id : "";
    bridge.world.folders = { [id]: "/Users/you/code/lamphaus/src" };
    await userEvent.click(await screen.findByRole("treeitem", { name: /^Shell: build/ }));
    await userEvent.keyboard("{Shift>}F{/Shift}");
    await userEvent.click(
      await screen.findByRole("button", { name: "File under lamphaus, as it is" }),
    );
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        FileSession: { session: id, workspace: ulid(20) },
      }),
    );
  });

  it("moves a project to another space from its page's menu, or to a new one", async () => {
    const bridge = new MockBridge(workingDay());
    await bridge.command({ CreateSpace: { name: "Home" } });
    await bridge.command({ CreateSpace: { name: "Work" } });
    const [home, work] = (bridge.world.spaces ?? []).map((space) => space.id);
    await bridge.command({ MoveToSpace: { space: home ?? "", member: { Project: lamphaus } } });
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    await userEvent.click(await screen.findByRole("treeitem", { name: "lamphaus" }));
    const page = await screen.findByRole("region", { name: "lamphaus" });
    const choose = async (item: RegExp) => {
      await userEvent.click(within(page).getByRole("button", { name: "More" }));
      const menu = await screen.findByRole("menu", { name: "lamphaus" });
      expect(within(menu).queryByRole("menuitem", { name: /^Move to Home/ })).toBeNull();
      await userEvent.click(await within(menu).findByRole("menuitem", { name: item }));
    };
    await choose(/^Move to Work/);
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        MoveToSpace: { space: work, member: { Project: lamphaus } },
      }),
    );
    await waitFor(() =>
      expect(bridge.world.spaces?.find((space) => space.id === home)?.members).toEqual([]),
    );
    await userEvent.click(within(page).getByRole("button", { name: "More" }));
    const menu = await screen.findByRole("menu", { name: "lamphaus" });
    await userEvent.click(within(menu).getByRole("menuitem", { name: /^Move to a new space/ }));
    const named = await screen.findByRole("dialog", { name: "New space" });
    await userEvent.type(within(named).getByLabelText("Name"), "Client");
    await userEvent.click(within(named).getByRole("button", { name: /Make it/ }));
    await waitFor(() =>
      expect(bridge.world.spaces?.find((space) => space.name === "Client")?.members).toEqual([
        { Project: lamphaus },
      ]),
    );
  });

  it("a space heading's menu adds to it, and its removal is refused while it holds anything", async () => {
    const bridge = new MockBridge(workingDay());
    await bridge.command({ CreateSpace: { name: "Home" } });
    const home = bridge.world.spaces?.[0]?.id ?? "";
    await bridge.command({ MoveToSpace: { space: home, member: { Project: lamphaus } } });
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    await screen.findByRole("treeitem", { name: "Home" });
    rightClick("Home");
    const menu = await screen.findByRole("menu", { name: "Home" });
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent?.replace(/[a-z+]$/, "")),
    ).toEqual(["Open a folder…", "Clone from GitHub…", "Terminal", "Rename Home…", "Remove Home…"]);
    await userEvent.click(within(menu).getByRole("menuitem", { name: /^Terminal/ }));
    await waitFor(() =>
      expect(bridge.world.spaces?.[0]?.members).toContainEqual(
        expect.objectContaining({ Session: expect.any(String) }),
      ),
    );
    rightClick("Home");
    await userEvent.click(
      within(await screen.findByRole("menu", { name: "Home" })).getByRole("menuitem", {
        name: /^Remove Home/,
      }),
    );
    const asked = await screen.findByRole("dialog", { name: "Remove Home?" });
    await userEvent.click(within(asked).getByRole("button", { name: /Remove the space/ }));
    expect(await within(asked).findByRole("alert")).toHaveTextContent(
      "Home still holds 1 project and 1 terminal: move or remove them first",
    );
    expect(bridge.world.spaces).toHaveLength(1);
  });

  it("opens on everything, whatever space a window showed before", async () => {
    localStorage.setItem("muster.space", JSON.stringify("spc_01JD0000000000000000000701"));
    const bridge = new MockBridge(workingDay());
    await bridge.command({ CreateSpace: { name: "Work" } });
    expect(createUi().store.getState().space).toBeNull();
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    // Every space's heading, with what is in none after them.
    expect(await screen.findByRole("treeitem", { name: "Work" })).toBeInTheDocument();
    expect(screen.getByRole("treeitem", { name: "muster" })).toBeInTheDocument();
  });

  it("notes a free terminal's lack of a project", async () => {
    const { app, bridge } = await shown();
    const reply = await act(() => startTerminal(bridge, "notes", null));
    const id =
      typeof reply === "object" && "SessionStarted" in reply ? reply.SessionStarted.id : "";
    await userEvent.click(await screen.findByRole("treeitem", { name: /^Shell: notes/ }));
    expect(app.ui.getState().selected.all).toBe(id);
    expect(await screen.findByText("no project: no undo, no changes view")).toBeInTheDocument();
  });
});
