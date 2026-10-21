import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockBridge } from "../ipc/mock/bridge";
import { ulid, workingDay } from "../ipc/mock/scenario";
import { renderWithApp } from "../test/render";
import { SessionPane } from "./SessionPane";
import { ThisTurn } from "./ThisTurn";

const codex = ulid(101);
const desktop = ulid(11);

describe("ARC-24 the session pane", () => {
  it("says which session, where it runs and what it uses (PRD-12, PRD-05)", async () => {
    renderWithApp(<SessionPane session={codex} />);
    expect(
      await screen.findByRole("heading", { name: "Sketch the Tauri shell" }),
    ).toBeInTheDocument();
    const pane = screen.getByRole("region", { name: "Sketch the Tauri shell" });
    expect(pane).toHaveTextContent("⎇ feature/desktop-app in muster");
    expect(pane).toHaveTextContent("Codex");
    expect(pane).toHaveTextContent("182k tokens");
    expect(pane).toHaveTextContent("212 MB, 3% CPU");
    expect(pane).toHaveTextContent("#91");
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

  it("opens the changes on Changes, and offers the shell and full screen with their keys", async () => {
    const { app } = renderWithApp(<SessionPane session={codex} />);
    await screen.findByRole("heading", { name: "Sketch the Tauri shell" });
    expect(screen.getByRole("button", { name: "Shell" })).toHaveAttribute(
      "title",
      "Shell in this workspace (⇧T)",
    );
    expect(screen.getByRole("button", { name: "Full screen" })).toHaveAttribute(
      "title",
      "Full screen (f)",
    );
    const changes = screen.getByRole("button", { name: "Changes" });
    expect(changes).toHaveAttribute("title", "Changes (d)");
    await userEvent.click(changes);
    expect(app.ui.getState().pane).toBe("changes");
    expect(changes).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(changes);
    expect(app.ui.getState().pane).toBe("terminal");
  });

  it("offers the list's keys in its more menu, each with its key", async () => {
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
    const menu = screen.getByRole("menu", { name: "Sketch the Tauri shell" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual(["RenameR", "Closex"]);
    expect(items[0]).toHaveFocus();
    expect(within(menu).getByRole("separator")).toBeInTheDocument();
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(close).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.keyboard("R");
    expect(rename).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "More" })).toHaveFocus();
  });

  it("has no more menu where the list gives it nothing to offer", async () => {
    renderWithApp(<SessionPane session={codex} />);
    await screen.findByRole("heading", { name: "Sketch the Tauri shell" });
    expect(screen.queryByRole("button", { name: "More" })).not.toBeInTheDocument();
  });
});

describe("ARC-25 This turn, read when the timeline changes", () => {
  it("shows the newest turn and its files, and opens its diff or putting it back (PRD-14)", async () => {
    const { app } = renderWithApp(<ThisTurn workspace={desktop} />);
    const turn = await screen.findByRole("region", { name: "This turn" });
    expect(await within(turn).findByText("Codex, turn 3, changed 3 files")).toBeInTheDocument();
    expect(turn).toHaveTextContent("+203−4");
    const pills = await within(turn).findAllByRole("button", { name: /its diff$/ });
    expect(pills.map((pill) => pill.textContent)).toEqual([
      "link.rs+118−4",
      "main.rs+61",
      "Cargo.toml+24",
    ]);
    expect(pills[0]).toHaveAttribute("title", "crates/muster-desktop/src/link.rs");
    await userEvent.click(within(turn).getByRole("button", { name: "Put back" }));
    expect(app.ui.getState()).toMatchObject({
      pane: "changes",
      changesTurn: { workspace: desktop, number: 4, intent: "put-back" },
    });
    await userEvent.click(within(turn).getByRole("button", { name: "See the diff" }));
    expect(app.ui.getState().changesTurn).toMatchObject({ number: 4, intent: "diff" });
    await userEvent.click(pills[1] as HTMLElement);
    expect(app.ui.getState().changesTurn).toMatchObject({ number: 4, intent: "diff" });
  });

  it("PRD-14 keeps Put back from a turn while an agent works there", async () => {
    const { bridge } = renderWithApp(<ThisTurn workspace={desktop} />);
    const putBack = await screen.findByRole("button", { name: "Put back" });
    expect(putBack).toBeEnabled();
    act(() => bridge.setStatus(codex, { state: "running" }));
    expect(putBack).toBeDisabled();
    expect(putBack).toHaveAttribute("title", "Wait for its turn to end");
  });

  it("says it is reading, what went wrong, and reads again on Try again", async () => {
    const bridge = new MockBridge(workingDay());
    const command = { Timeline: { workspace: desktop } };
    bridge.hold(command);
    bridge.failNext(command, "the timeline is busy: try again in a moment");
    renderWithApp(<ThisTurn workspace={desktop} />, bridge);
    expect(screen.getByRole("status")).toHaveTextContent("Reading the timeline…");
    act(() => bridge.release(command));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The timeline could not be read: the timeline is busy: try again in a moment",
    );
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Codex, turn 3, changed 3 files")).toBeInTheDocument();
  });

  it("says none is kept yet in a workspace with no turns, as the terminal UI does", async () => {
    renderWithApp(<ThisTurn workspace={ulid(12)} />);
    expect(
      await screen.findByText("None kept yet: each agent's turn here is, from now on."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Put back" })).not.toBeInTheDocument();
  });

  it("says a turn changed no files, with no pills", async () => {
    const bridge = new MockBridge(workingDay());
    const newest = bridge.world.timelines?.[desktop]?.entries[0];
    if (newest) newest.summary = null;
    renderWithApp(<ThisTurn workspace={desktop} />, bridge);
    expect(await screen.findByText("Codex, turn 3, changed no files")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /its diff$/ })).not.toBeInTheDocument();
  });

  it("reads again when the daemon tells of a newer entry, and asks nothing while hidden", async () => {
    const { app, bridge } = renderWithApp(<ThisTurn workspace={desktop} />);
    await screen.findByRole("button", { name: /link\.rs, its diff/ });
    const timelines = () =>
      bridge.sent.filter((sent) => typeof sent === "object" && "Timeline" in sent);
    expect(timelines()).toHaveLength(1);
    // Hidden and shown again with nothing new: nothing asked again.
    act(() => app.actions.setVisible(false));
    act(() => app.actions.setVisible(true));
    expect(timelines()).toHaveLength(1);
    // A new entry while hidden is read once the window is seen.
    act(() => app.actions.setVisible(false));
    const entries = bridge.world.timelines?.[desktop]?.entries ?? [];
    const first = entries[0];
    if (first)
      entries.unshift({ ...first, number: 5, summary: { files: 1, added: 1, removed: 0 } });
    act(() => bridge.updateWorkspace(desktop, { timeline: 5 }));
    await act(async () => {});
    expect(timelines()).toHaveLength(1);
    act(() => app.actions.setVisible(true));
    expect(await screen.findByText("Codex, turn 4, changed 1 file")).toBeInTheDocument();
    expect(timelines()).toHaveLength(2);
  });

  it("keeps the turn when its files cannot be read, and says a diff cut short", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.failNext({ EntryDiff: { workspace: desktop, number: 4 } }, "the diff is gone");
    renderWithApp(<ThisTurn workspace={desktop} />, bridge);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Its files could not be read: the diff is gone",
    );
    expect(screen.getByText("Codex, turn 3, changed 3 files")).toBeInTheDocument();
    const diff = bridge.world.entryDiffs?.[desktop]?.[4];
    if (diff) diff.cut = true;
    act(() => bridge.updateWorkspace(desktop, { timeline: 4.5 }));
    await waitFor(() =>
      expect(
        screen.getByText("The diff was cut short; it changed more files than these."),
      ).toBeInTheDocument(),
    );
  });
});
