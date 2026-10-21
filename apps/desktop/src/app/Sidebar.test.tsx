import { act, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

describe("ARC-24 the sidebar", () => {
  it("lists the terminal UI's tabs in its order, then the projects", async () => {
    renderApp();
    const nav = await screen.findByRole("navigation", { name: "Screens" });
    await within(nav).findByRole("button", { name: /^muster/ });
    const names = within(nav)
      .getAllByRole("button")
      .map((button) => button.textContent?.replace(/^[≡▲#]/, "").replace(/\d+$/, ""));
    expect(names.slice(0, 4)).toEqual(["Sessions", "Needs you", "Tickets", "Settings"]);
    expect(names).toContain("Add a project");
  });

  it("counts what waits on the person beside Needs you", async () => {
    const { bridge } = renderApp();
    const needsYou = await screen.findByRole("button", { name: /Needs you/ });
    expect(needsYou).toHaveTextContent("Needs you2");
    act(() => bridge.setStatus(ulid(201), { state: "needs_you", reason: "question" }));
    expect(needsYou).toHaveTextContent("Needs you3");
  });
});
