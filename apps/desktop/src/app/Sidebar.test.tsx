import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

// The one sidebar (PRD §6a): a new session, the palette and Tickets, the
// switch between what needs the person and every session with the list
// itself under it, and Settings at its foot.

const sidebar = () => screen.findByRole("navigation", { name: "Sessions" });

describe("ARC-24 the sidebar", () => {
  it("has a new session, the palette and Tickets, the lists' switch, and Settings at its foot", async () => {
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

  it("starts a new session from its button, where the selected session works", async () => {
    renderApp();
    await screen.findByRole("option", { name: "Sketch the Tauri shell" });
    await userEvent.click(screen.getByRole("button", { name: /^New session/ }));
    expect(await screen.findByRole("dialog", { name: /New session/ })).toBeInTheDocument();
  });
});
