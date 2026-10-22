import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

// The one sidebar (PRD §6a): a new session, the palette and Tickets, the
// switch between what needs the person and every session, the projects'
// heading with the way to add one, the list itself under it, and Settings
// at its foot.

const sidebar = () => screen.findByRole("navigation", { name: "Sessions" });

describe("ARC-24 the sidebar", () => {
  it("has a new session, the palette and Tickets, the lists' switch, a way to add a project, and Settings at its foot", async () => {
    renderApp();
    const nav = await sidebar();
    await within(nav).findByRole("option", { name: "Sketch the Tauri shell" });
    const buttons = within(nav)
      .getAllByRole("button")
      .filter((button) => !button.closest('[role="listbox"]'))
      .map((button) => button.textContent?.replace(/⌘.$/, "").replace(/\d+$/, "") || button.title);
    expect(buttons).toEqual([
      "Hide the sidebar (⌘\\)",
      "New session",
      "Search",
      "Tickets",
      "Needs you",
      "All",
      "Add a project (a)",
      "Settings",
    ]);
  });

  it("counts what waits on the person beside Needs you, and every chat beside All", async () => {
    const { bridge } = renderApp();
    const needsYou = await screen.findByRole("button", { name: /^Needs you/ });
    expect(needsYou).toHaveTextContent("Needs you2");
    expect(screen.getByRole("button", { name: /^All/ })).toHaveTextContent("All8");
    act(() => bridge.setStatus(ulid(201), { state: "needs_you", reason: "question" }));
    expect(needsYou).toHaveTextContent("Needs you3");
  });

  it("switches the list between Needs you and every session by project", async () => {
    renderApp();
    const needsYou = await screen.findByRole("button", { name: /^Needs you/ });
    expect(needsYou).toHaveAttribute("aria-pressed", "true");
    await screen.findByRole("listbox", { name: "Needs you sessions" });
    await userEvent.click(screen.getByRole("button", { name: /^All/ }));
    expect(await screen.findByRole("tree", { name: "Projects and sessions" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^All/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /^Needs you/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.queryByRole("listbox", { name: "Needs you sessions" })).toBeNull();
  });

  it("keeps the list beside Tickets and Settings, and a session picked there is shown in their place", async () => {
    renderApp();
    await screen.findByRole("option", { name: "Sketch the Tauri shell" });
    await userEvent.click(screen.getByRole("button", { name: "Tickets" }));
    expect(await screen.findByRole("heading", { name: "Tickets", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tickets" })).toHaveAttribute("aria-current", "page");
    await userEvent.click(screen.getByRole("option", { name: "Explain the release script" }));
    expect(
      await screen.findByRole("heading", { name: "Explain the release script", level: 2 }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tickets", level: 1 })).toBeNull();
  });

  it("New session opens the selected session's project page, its box ready", async () => {
    renderApp();
    await screen.findByRole("option", { name: "Sketch the Tauri shell" });
    await userEvent.click(screen.getByRole("button", { name: /^New session/ }));
    const page = await screen.findByRole("region", { name: "muster" });
    expect(within(page).getByRole("textbox", { name: "First prompt" })).toHaveFocus();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("adds a project from the Mac's own folder panel, the folder browser standing in where there is none", async () => {
    const { bridge } = renderApp();
    await screen.findByRole("option", { name: "Sketch the Tauri shell" });
    const add = screen.getByRole("button", { name: /^Add a project/ });

    bridge.picked = { kind: "cancelled" };
    await userEvent.click(add);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      bridge.sent.some((command) => typeof command === "object" && "AddProject" in command),
    ).toBe(false);

    bridge.picked = { kind: "chosen", path: "/Users/you/code/labs" };
    await userEvent.click(add);
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        AddProject: { root: "/Users/you/code/labs", name: null },
      }),
    );
    expect(await screen.findByRole("region", { name: "labs" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();

    bridge.picked = { kind: "unavailable" };
    await userEvent.click(screen.getByRole("button", { name: /^Add a project/ }));
    expect(await screen.findByRole("dialog", { name: "Add a project" })).toBeInTheDocument();
  });
});
