import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp } from "../test/render";

describe("the window", () => {
  it("counts the flock as the daemon derives it", async () => {
    renderApp();
    expect(
      await screen.findByRole("img", { name: "4 working, 2 waiting on you, 2 done" }),
    ).toBeInTheDocument();
  });

  it("lists the projects in the daemon's order, each in its colour", async () => {
    renderApp();
    const names = await screen.findAllByText(/^(muster|lamphaus|kep-site|kep-api)$/);
    expect(names.map((name) => name.textContent)).toEqual([
      "muster",
      "lamphaus",
      "kep-site",
      "kep-api",
    ]);
  });

  it("goes between screens from the sidebar", async () => {
    renderApp();
    await userEvent.click(await screen.findByRole("button", { name: /Tickets/ }));
    expect(await screen.findByRole("heading", { name: "Tickets" })).toBeInTheDocument();
  });

  it("goes between screens by key", async () => {
    renderApp();
    await screen.findByRole("heading", { name: "Needs you" });
    await userEvent.keyboard("{Meta>},{/Meta}");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("follows a status the daemon tells", async () => {
    const { bridge } = renderApp();
    await screen.findByRole("img", { name: "4 working, 2 waiting on you, 2 done" });
    act(() => bridge.setStatus(ulid(100), { state: "needs_you", reason: "question" }));
    await waitFor(() =>
      expect(
        screen.getByRole("img", { name: "3 working, 3 waiting on you, 2 done" }),
      ).toBeInTheDocument(),
    );
  });

  it("shows Kep away while the daemon is out of reach", async () => {
    const { bridge } = renderApp();
    await screen.findByRole("img", { name: "Something asks for you" });
    act(() => bridge.setLink({ state: "down", reason: "no daemon", since: 0 }));
    expect(
      await screen.findByRole("img", { name: "The daemon is out of reach" }),
    ).toBeInTheDocument();
  });
});
