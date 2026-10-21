import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp, renderWithApp } from "../test/render";
import { StatusBar } from "./StatusBar";

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

  it("out of reach, Try now asks the link at once and Show its log opens the log, or says why not", async () => {
    const { bridge } = renderApp();
    await screen.findByRole("img", { name: "Something asks for you" });
    act(() => bridge.setLink({ state: "down", reason: "no daemon", since: 0 }));
    const banner = await screen.findByRole("alert");
    await userEvent.click(within(banner).getByRole("button", { name: "Try now" }));
    expect(bridge.triedNow).toBe(1);
    await userEvent.click(within(banner).getByRole("button", { name: "Show its log" }));
    expect(bridge.logsShown).toBe(1);
    bridge.showDaemonLog = () =>
      Promise.reject(new Error("There is no daemon.log yet in /tmp/mu."));
    await userEvent.click(within(banner).getByRole("button", { name: "Show its log" }));
    expect(await screen.findByText("There is no daemon.log yet in /tmp/mu.")).toBeInTheDocument();
  });

  it("says the status bar's notes to a screen reader from a region always there, never twice", async () => {
    const { app } = renderWithApp(<StatusBar />);
    const bar = screen.getByRole("contentinfo");
    const said = bar.querySelector('[data-said="note"]') as HTMLElement;
    expect(said).toHaveAttribute("aria-live", "polite");
    expect(said).toBeEmptyDOMElement();
    act(() => app.actions.tell("Renamed it to Sketch."));
    expect(said).toHaveTextContent("Renamed it to Sketch.");
    expect(within(bar).getAllByText("Renamed it to Sketch.")).toHaveLength(1);
    act(() => app.actions.tell("The daemon refused it.", true));
    expect(said).toHaveAttribute("aria-live", "assertive");
    expect(said).toHaveTextContent("The daemon refused it.");
  });

  it("folds the sidebar on the person's fold_list key as on ⌘\\", async () => {
    renderApp();
    const sidebar = await screen.findByRole("navigation", { name: "Screens" });
    await screen.findByText("Projects");
    await userEvent.keyboard("{Control>}\\{/Control}");
    await waitFor(() => expect(sidebar).toHaveClass("mu-nav--folded"));
    await userEvent.keyboard("{Control>}\\{/Control}");
    await waitFor(() => expect(sidebar).not.toHaveClass("mu-nav--folded"));
  });
});
