// Spaces and terminals in no project in the window (`ARC-33`, PRD §6a): the
// All list shows the space the window chose, as the model lists it; the
// projects' heading becomes the way among spaces once there is one; a
// terminal in no project is listed under Terminals; and its menu puts it in
// a space, or under a project where its shell is.

import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../app/keys";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
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

describe("ARC-33 spaces in the window", () => {
  it("lists only the space shown, and everything again", async () => {
    const { app, bridge } = await shown();
    await act(() => bridge.command({ CreateSpace: { name: "Home" } }));
    const space = bridge.world.spaces?.[0]?.id ?? "";
    await act(() => bridge.command({ AddToSpace: { space, member: { Project: lamphaus } } }));
    expect(row("muster")).toBeInTheDocument();
    act(() => app.actions.showSpace(space));
    await waitFor(() => expect(screen.queryByRole("treeitem", { name: "muster" })).toBeNull());
    expect(row("lamphaus")).toBeInTheDocument();
    act(() => app.actions.showSpace(null));
    await waitFor(() => expect(row("muster")).toBeInTheDocument());
  });

  it("lists a terminal in no project under Terminals, and folds them away", async () => {
    const { bridge } = await shown();
    await act(() =>
      bridge.command({
        StartSession: {
          workspace: null,
          harness: "shell",
          title: "vm",
          size: { cols: 80, rows: 24 },
          prompt: null,
          started_by: null,
          space: null,
        },
      }),
    );
    const heading = await screen.findByRole("treeitem", { name: "Terminals in no project" });
    expect(row(/^Shell: vm/)).toBeInTheDocument();
    await userEvent.click(heading);
    await userEvent.keyboard("e");
    await waitFor(() => expect(screen.queryByRole("treeitem", { name: /^Shell: vm/ })).toBeNull());
  });

  it("names the space shown in the projects' heading once there is one", async () => {
    const { bridge } = renderApp();
    await screen.findByRole("navigation", { name: "Sessions" });
    expect(screen.queryByRole("button", { name: "Spaces" })).toBeNull();
    await act(() => bridge.command({ CreateSpace: { name: "Client" } }));
    const trigger = await screen.findByRole("button", { name: "Spaces" });
    expect(trigger).toHaveTextContent("Everything");
    await userEvent.click(trigger);
    const menu = await screen.findByRole("menu", { name: "Spaces" });
    await userEvent.click(within(menu).getByRole("menuitem", { name: /^Client/ }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Spaces" })).toHaveTextContent("Client"),
    );
  });

  it("files a terminal in no project under the project its shell is in", async () => {
    const bridge = new MockBridge(workingDay());
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    const reply = await act(() =>
      bridge.command({
        StartSession: {
          workspace: null,
          harness: "shell",
          title: "build",
          size: { cols: 80, rows: 24 },
          prompt: null,
          started_by: null,
          space: null,
        },
      }),
    );
    const id =
      typeof reply === "object" && "SessionStarted" in reply ? reply.SessionStarted.id : "";
    bridge.world.folders = { [id]: "/Users/you/code/lamphaus/src" };
    await userEvent.click(screen.getByRole("button", { name: /^All/ }));
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

  it("puts a project in a space from its page's menu, takes it out, and makes a new space with it in", async () => {
    const bridge = new MockBridge(workingDay());
    renderApp(bridge);
    await screen.findByRole("navigation", { name: "Sessions" });
    await act(() => bridge.command({ CreateSpace: { name: "Home" } }));
    const home = bridge.world.spaces?.[0]?.id ?? "";
    await userEvent.click(screen.getByRole("button", { name: /^All/ }));
    await userEvent.click(await screen.findByRole("treeitem", { name: "lamphaus" }));
    const page = await screen.findByRole("region", { name: "lamphaus" });
    const choose = async (item: RegExp) => {
      await userEvent.click(within(page).getByRole("button", { name: "More" }));
      const menu = await screen.findByRole("menu", { name: "lamphaus" });
      await userEvent.click(await within(menu).findByRole("menuitem", { name: item }));
    };
    await choose(/^Put in Home/);
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        AddToSpace: { space: home, member: { Project: lamphaus } },
      }),
    );
    await choose(/^Take out of Home/);
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        RemoveFromSpace: { space: home, member: { Project: lamphaus } },
      }),
    );
    await choose(/^Put in a new space/);
    const named = await screen.findByRole("dialog", { name: "New space" });
    await userEvent.type(within(named).getByLabelText("Name"), "Client");
    await userEvent.click(within(named).getByRole("button", { name: /Make it/ }));
    await waitFor(() =>
      expect(bridge.world.spaces?.find((space) => space.name === "Client")?.members).toEqual([
        { Project: lamphaus },
      ]),
    );
  });

  it("notes a project's spaces while everything is shown, and a free terminal's lack of a project", async () => {
    const { app, bridge } = await shown();
    await act(() => bridge.command({ CreateSpace: { name: "Home" } }));
    const space = bridge.world.spaces?.[0]?.id ?? "";
    await act(() => bridge.command({ AddToSpace: { space, member: { Project: lamphaus } } }));
    await waitFor(() => expect(row("lamphaus")).toHaveTextContent("in Home"));
    act(() => app.actions.showSpace(space));
    await waitFor(() => expect(row("lamphaus")).not.toHaveTextContent("in Home"));
    act(() => app.actions.showSpace(null));
    const reply = await act(() =>
      bridge.command({
        StartSession: {
          workspace: null,
          harness: "shell",
          title: "notes",
          size: { cols: 80, rows: 24 },
          prompt: null,
          started_by: null,
          space: null,
        },
      }),
    );
    const id =
      typeof reply === "object" && "SessionStarted" in reply ? reply.SessionStarted.id : "";
    await userEvent.click(await screen.findByRole("treeitem", { name: /^Shell: notes/ }));
    expect(app.ui.getState().selected.all).toBe(id);
    expect(await screen.findByText("no project: no undo, no changes view")).toBeInTheDocument();
  });
});
