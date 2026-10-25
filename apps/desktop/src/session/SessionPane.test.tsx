import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ulid } from "../ipc/mock/scenario";
import { renderApp, renderWithApp } from "../test/render";
import { tooltipOf } from "../test/tooltip";
import { SessionPane } from "./SessionPane";

const codex = ulid(101);

describe("ARC-24 the session pane", () => {
  it("says in one row which session, where it runs and what its work there comes to", async () => {
    renderWithApp(<SessionPane session={codex} />);
    expect(
      await screen.findByRole("heading", { name: "Sketch the Tauri shell" }),
    ).toBeInTheDocument();
    const head = screen.getByRole("region", { name: "Sketch the Tauri shell" });
    expect(head).toHaveTextContent("⎇ feature/desktop-app in muster");
    expect(head).toHaveTextContent("+300−6");
    expect(head).toHaveTextContent("overlap");
    expect(head).toHaveTextContent("#91");
  });

  it("PRD-12 says its agent and what it uses at the status bar's end, for the session shown", async () => {
    renderApp();
    await userEvent.click(
      await screen.findByRole("treeitem", { name: /^Sketch the Tauri shell,/ }),
    );
    await screen.findByRole("heading", { name: "Sketch the Tauri shell", level: 2 });
    expect(screen.getByRole("contentinfo")).toHaveTextContent(
      "Codex · 182k tokens · 212 MB, 3% CPU",
    );
    await userEvent.click(screen.getByRole("button", { name: "Tickets" }));
    await screen.findByRole("heading", { name: "Tickets", level: 1 });
    expect(screen.getByRole("contentinfo")).not.toHaveTextContent("182k tokens");
  });

  it("has nothing under the terminal: the turns are in the changes", async () => {
    renderWithApp(<SessionPane session={codex} />);
    await screen.findByRole("heading", { name: "Sketch the Tauri shell" });
    expect(screen.queryByRole("region", { name: "This turn" })).not.toBeInTheDocument();
  });

  it("shows the needs-you banner only while the session asks", async () => {
    const { bridge } = renderWithApp(<SessionPane session={codex} />);
    const banner = await screen.findByRole("status", { name: "" });
    expect(banner).toHaveTextContent("Needs you: wants to run cargo build");
    expect(banner).toHaveTextContent("1m");
    expect(banner).toHaveTextContent("⏎answer in the terminal");
    act(() => bridge.setStatus(codex, { state: "running" }));
    expect(screen.queryByText("Needs you:")).not.toBeInTheDocument();
    act(() => bridge.setStatus(codex, { state: "needs_you", reason: "question" }));
    expect(screen.getByText("Needs you:").parentElement).toHaveTextContent(
      "wants to run cargo build",
    );
  });

  it("offers the shell and the changes as icons, each with its key in its tooltip", async () => {
    const { app } = renderWithApp(<SessionPane session={codex} />);
    await screen.findByRole("heading", { name: "Sketch the Tauri shell" });
    const shell = screen.getByRole("button", { name: "Shell" });
    expect(await tooltipOf(shell)).toBe("Shell in this workspace (⇧T)");
    expect(shell).toHaveTextContent("");
    expect(screen.queryByRole("button", { name: "Full screen" })).not.toBeInTheDocument();
    const changes = screen.getByRole("button", { name: "Changes" });
    expect(await tooltipOf(changes)).toBe("Changes (d)");
    expect(changes).toHaveTextContent("");
    await userEvent.click(changes);
    expect(app.ui.getState().pane).toBe("changes");
    expect(changes).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(changes);
    expect(app.ui.getState().pane).toBe("terminal");
  });

  it("offers full screen and the list's keys in its more menu, each with its key", async () => {
    const rename = vi.fn();
    const close = vi.fn();
    renderWithApp(
      <SessionPane
        session={codex}
        menu={[
          { key: "R", label: "Rename", does: rename },
          { key: "x", label: "Close", does: close, danger: true },
        ]}
      />,
    );
    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    const menu = await screen.findByRole("menu", { name: "Sketch the Tauri shell" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual(["Full screenf", "RenameR", "Closex"]);
    // Opened by the pointer, the menu holds the focus with no item lit, as a Mac's does; ↓ lights the first.
    await waitFor(() => expect(menu).toHaveFocus());
    expect(within(menu).getByRole("separator")).toBeInTheDocument();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{Enter}");
    expect(close).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    const again = await screen.findByRole("menu");
    await waitFor(() => expect(again).toHaveFocus());
    await userEvent.keyboard("R");
    expect(rename).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await screen.findByRole("menu");
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "More" })).toHaveFocus();
  });

  it("fills the window from its more menu, and says so there to leave it", async () => {
    const { app } = renderWithApp(<SessionPane session={codex} />);
    await userEvent.click(await screen.findByRole("button", { name: "More" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Full screen/ }));
    expect(app.ui.getState().fullScreen).toBe(codex);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Leave full screen/ }));
    expect(app.ui.getState().fullScreen).toBeNull();
  });
});
