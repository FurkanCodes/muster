import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

// The one sidebar (PRD §6a): a new session, the palette, Tickets and Needs
// you under it, the projects' heading with the way to add one, the list of
// every session itself under it, and Settings at its foot.

const sidebar = () => screen.findByRole("navigation", { name: "Sessions" });

describe("ARC-24 the sidebar", () => {
  it("has a new session, the palette, Tickets, the Needs you and All chip, a way to add a project, and Settings at its foot", async () => {
    renderApp();
    const nav = await sidebar();
    await within(nav).findByRole("treeitem", { name: /^Sketch the Tauri shell,/ });
    const buttons = within(nav)
      .getAllByRole("button")
      .filter((button) => !button.closest('[role="tree"]'))
      .map(
        (button) =>
          button.textContent?.replace(/⌘.$/, "").replace(/\d+$/, "") ||
          button.getAttribute("aria-label"),
      );
    expect(buttons).toEqual([
      "Hide the sidebar",
      "New session",
      "Search",
      "Tickets",
      "Needs you",
      "All",
      "Terminal",
      "New space",
      "Settings",
    ]);
  });

  it("opens on All, and counts what waits on the person beside Needs you", async () => {
    const { bridge } = renderApp();
    const needsYou = await screen.findByRole("button", { name: /^Needs you/ });
    expect(screen.getByRole("button", { name: /^All/ })).toHaveAttribute("aria-pressed", "true");
    expect(needsYou).toHaveTextContent("Needs you2");
    act(() => bridge.setStatus(ulid(201), { state: "needs_you", reason: "question" }));
    expect(needsYou).toHaveTextContent("Needs you3");
  });

  it("opens Needs you beside the list of everything, and an entry there opens its session in the list", async () => {
    renderApp();
    const tree = await screen.findByRole("tree", { name: "Projects and sessions" });
    await userEvent.click(screen.getByRole("button", { name: /^Needs you/ }));
    expect(await screen.findByRole("heading", { name: "Needs you", level: 1 })).toBeVisible();
    expect(screen.getByRole("button", { name: /^Needs you/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    // The sidebar still lists everything.
    expect(tree).toBeVisible();
    const queue = screen.getByRole("listbox", { name: "Needs you sessions" });
    await userEvent.click(within(queue).getByRole("option", { name: "Sketch the Tauri shell" }));
    expect(
      await screen.findByRole("heading", { name: "Sketch the Tauri shell", level: 2 }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Needs you", level: 1 })).toBeNull();
    expect(screen.getByRole("treeitem", { name: /^Sketch the Tauri shell,/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("keeps the list beside Tickets, and a session picked there is shown in their place", async () => {
    renderApp();
    await screen.findByRole("treeitem", { name: /^Sketch the Tauri shell,/ });
    await userEvent.click(screen.getByRole("button", { name: "Tickets" }));
    expect(await screen.findByRole("heading", { name: "Tickets", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tickets" })).toHaveAttribute("aria-current", "page");
    await userEvent.click(screen.getByRole("treeitem", { name: /^Explain the release script,/ }));
    expect(
      await screen.findByRole("heading", { name: "Explain the release script", level: 2 }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tickets", level: 1 })).toBeNull();
  });

  it("goes from Settings to Needs you, and back to the list on a click on a line", async () => {
    renderApp();
    await screen.findByRole("treeitem", { name: /^Sketch the Tauri shell,/ });
    await userEvent.click(screen.getByRole("button", { name: /^Settings/ }));
    await userEvent.click(screen.getByRole("button", { name: /^Needs you/ }));
    expect(await screen.findByRole("heading", { name: "Needs you", level: 1 })).toBeVisible();
    expect(screen.getByRole("button", { name: /^Settings/ })).not.toHaveAttribute(
      "aria-current",
      "page",
    );
    await userEvent.click(screen.getByRole("treeitem", { name: /^Sketch the Tauri shell,/ }));
    expect(screen.queryByRole("heading", { name: "Needs you", level: 1 })).toBeNull();
  });

  it("New session opens the selected session's project page, its box ready", async () => {
    renderApp();
    await screen.findByRole("treeitem", { name: /^Sketch the Tauri shell,/ });
    await userEvent.click(screen.getByRole("button", { name: /^New session/ }));
    const page = await screen.findByRole("region", { name: "muster" });
    expect(within(page).getByRole("textbox", { name: "First prompt" })).toHaveFocus();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("adds a project from the Mac's own folder panel, the folder browser standing in where there is none", async () => {
    const bridge = new MockBridge(workingDay());
    // A space to add to, as there is once anything was added (`ARC-33`).
    bridge.world.spaces = [{ id: ulid(700), name: "Home", members: [], created_at: 0 }];
    renderApp(bridge);
    await screen.findByRole("treeitem", { name: /^Sketch the Tauri shell,/ });
    // A space's + offers a folder of the Mac's, a clone from GitHub, or a terminal.
    const add = async () => {
      await userEvent.click(screen.getByRole("button", { name: "Add to Home" }));
      const menu = await screen.findByRole("menu", { name: "Add to Home" });
      expect(
        within(menu)
          .getAllByRole("menuitem")
          .map((item) => item.textContent),
      ).toEqual(["Folder or project…a", "Clone a GitHub repository…g", "Terminal"]);
      await userEvent.click(within(menu).getByRole("menuitem", { name: /Folder or project/ }));
    };

    bridge.picked = { kind: "cancelled" };
    await add();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      bridge.sent.some((command) => typeof command === "object" && "AddProject" in command),
    ).toBe(false);

    bridge.picked = { kind: "chosen", path: "/Users/you/code/labs" };
    await add();
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        AddProject: { root: "/Users/you/code/labs", name: null, space: ulid(700) },
      }),
    );
    expect(await screen.findByRole("region", { name: "labs" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();

    bridge.picked = { kind: "unavailable" };
    await add();
    expect(await screen.findByRole("dialog", { name: "Add a project" })).toBeInTheDocument();
  });
});
