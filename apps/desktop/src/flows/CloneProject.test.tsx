// Cloning a project from GitHub in the window, over the mock daemon: the
// repositories listed, chosen or named, then where, in the Mac's panel or,
// with none, a field; the daemon clones and adds it (PRD §6a).

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeAll, describe, expect, it } from "vitest";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

beforeAll(() => {
  // jsdom has no modal dialogs; WebKit's own are tested end to end.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});

/** A space to clone into, as there is once anything was added (`ARC-33`). */
const HOME = ulid(700);

async function cloneDialog(bridge = new MockBridge(workingDay())) {
  bridge.world.spaces = [{ id: HOME, name: "Home", members: [], created_at: 0 }];
  renderApp(bridge);
  await screen.findByRole("img", { name: /working, .* waiting on you/ });
  await userEvent.keyboard("g");
  const dialog = await screen.findByRole("dialog", { name: "Clone from GitHub" });
  return { bridge, dialog };
}

const clones = (bridge: MockBridge) =>
  bridge.sent.filter((command) => typeof command === "object" && "CloneProject" in command);

describe("PRD §6a cloning a project from GitHub", () => {
  it("lists yours, narrows as you type, and clones into the folder the Mac's panel gives", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.picked = { kind: "chosen", path: "/Users/you/code" };
    const { dialog } = await cloneDialog(bridge);
    const listed = within(dialog).getByRole("listbox", { name: "Your repositories on GitHub" });
    await waitFor(() => expect(within(listed).getAllByRole("option")).toHaveLength(3));
    expect(within(listed).getByRole("option", { name: /you\/lamphaus/ })).toHaveTextContent(
      "private",
    );
    await userEvent.type(within(dialog).getByRole("combobox", { name: "Repository" }), "dot");
    expect(within(listed).getAllByRole("option")).toHaveLength(1);
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(clones(bridge)).toEqual([
        { CloneProject: { repository: "you/dotfiles", into: "/Users/you/code", space: HOME } },
      ]),
    );
    expect(bridge.pickedWith).toEqual(["Clone you/dotfiles into"]);
    // Then its page, and no dialog.
    expect(await screen.findByRole("region", { name: "dotfiles" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks where in a field where there is no panel, and clones a name typed though GitHub could not list", async () => {
    const bridge = new MockBridge(workingDay());
    const asked = bridge.command.bind(bridge);
    bridge.command = async (command) => {
      if (command === "ListRepositories")
        throw new Error("the GitHub CLI (gh) is not signed in; run `gh auth login`");
      return asked(command);
    };
    const { dialog } = await cloneDialog(bridge);
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("not signed in");
    await userEvent.keyboard("{Enter}");
    expect(await within(dialog).findByText(/Choose one, or type its name/)).toBeVisible();
    await userEvent.type(
      within(dialog).getByRole("combobox", { name: "Repository" }),
      "https://github.com/o/tool.git",
    );
    await userEvent.keyboard("{Enter}");
    const into = await within(dialog).findByRole("textbox", {
      name: /Clone into \(as tool\/ there\)/,
    });
    expect(into).toHaveValue("/Users/you");
    await userEvent.click(within(dialog).getByRole("button", { name: /^Clone o\/tool here/ }));
    await waitFor(() =>
      expect(clones(bridge)).toEqual([
        { CloneProject: { repository: "o/tool", into: "/Users/you", space: HOME } },
      ]),
    );
    expect(await screen.findByRole("region", { name: "tool" })).toBeInTheDocument();
  });

  it("says why it cannot clone where its folder is there already", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.picked = { kind: "chosen", path: "/Users/you" };
    const { dialog } = await cloneDialog(bridge);
    await userEvent.type(
      within(dialog).getByRole("combobox", { name: "Repository" }),
      "FurkanCodes/muster",
    );
    await userEvent.keyboard("{Enter}");
    expect(await within(dialog).findByText(/is there already/)).toBeVisible();
  });
});
