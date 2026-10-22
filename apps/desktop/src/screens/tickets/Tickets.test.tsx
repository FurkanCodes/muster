import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect } from "react";
import { beforeAll, describe, expect, it } from "vitest";
import { useApp } from "../../app/context";
import { installKeys } from "../../app/keys";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import Tickets from "./Tickets";

beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
function WithKeys() {
  const app = useApp();
  useEffect(() => installKeys(app), [app]);
  return <Tickets />;
}
/** The Tickets board's day: its agents on their tickets. */
function boardDay() {
  const bridge = new MockBridge(workingDay());
  bridge.agentsOnTickets();
  return bridge;
}
function renderTickets(bridge = boardDay()) {
  return renderWithApp(<WithKeys />, bridge);
}
const ticket = (key: string) => screen.getByRole("option", { name: new RegExp(`^#${key} `) });

describe("PRD-01 ARC-24 Tickets", () => {
  it("shows the board's grouped tickets, agents, labels, usage and text", async () => {
    const { bridge } = renderTickets();
    expect(await screen.findByRole("option", { name: /^#214 / })).toBeInTheDocument();
    expect(await screen.findByRole("article")).toHaveTextContent("Done when");
    expect(screen.getAllByText("182k tokens")).toHaveLength(2);
    expect(screen.getByText("opened by FurkanCodes")).toBeInTheDocument();
    expect(screen.getAllByRole("option", { name: /^KEP-31 / })).toHaveLength(1);
    expect(within(ticket("214")).getByRole("img", { name: "needs you" })).toBeInTheDocument();
    expect(within(ticket("214")).queryByRole("img", { name: "Shell" })).toBeNull();
    expect(bridge.sent.filter((c) => typeof c !== "string" && "ListIssues" in c)).toHaveLength(4);
  });
  it("shows loading and tracker errors, then refreshes with r", async () => {
    const bridge = boardDay();
    bridge.hold("ListIssues");
    bridge.failNext("ListIssues", "gh is not signed in");
    renderTickets(bridge);
    expect(await screen.findAllByRole("status", { name: "Fetching its tickets…" })).toHaveLength(4);
    act(() => bridge.release("ListIssues"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Its tickets could not be fetched: gh is not signed in",
    );
    await userEvent.keyboard("r");
    expect(await screen.findByRole("option", { name: /^#214 / })).toBeInTheDocument();
  });
  it("shows no open tickets, none of yours and the no-project state", async () => {
    const bridge = boardDay();
    for (const list of Object.values(bridge.world.tickets)) list.issues = [];
    const view = renderTickets(bridge);
    expect(await screen.findByText("none of yours open")).toBeInTheDocument();
    expect(screen.getAllByText("no open tickets")).toHaveLength(2);
    view.unmount();
    bridge.world.projects = [];
    bridge.world.workspaces = [];
    bridge.world.sessions = [];
    renderTickets(new MockBridge(bridge.world));
    expect(
      await screen.findByText("No projects yet: add one, and its open tickets show here."),
    ).toBeInTheDocument();
  });
  it("changes Only my tickets and says a refused change, the tickets read again", async () => {
    const { bridge, app } = renderTickets();
    await screen.findByRole("article");
    const mine = () =>
      within(screen.getByRole("group", { name: "Which of muster's tickets" })).getByRole("button", {
        name: "Mine",
      });
    bridge.failNext("ChangeSetting", "settings file cannot be written", "internal");
    await userEvent.click(mine());
    await waitFor(() =>
      expect(app.ui.getState().note).toEqual({
        text: "settings file cannot be written",
        error: true,
      }),
    );
    expect(await screen.findByRole("group", { name: "Which of muster's tickets" })).toBeVisible();
    expect(mine()).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(mine());
    await waitFor(() => expect(mine()).toHaveAttribute("aria-pressed", "true"));
    expect(screen.getAllByText("GitHub issues, only yours")).toHaveLength(2);
  });
  it("opens the newest live agent and remembers the ticket on return", async () => {
    const { app } = renderTickets();
    await screen.findByRole("article");
    await userEvent.keyboard("{Enter}");
    expect(app.ui.getState().screen).toBe("needs-you");
    expect(app.ui.getState().terminal).toBe(ulid(101));
    expect(app.ui.getState().selected.tickets).toBe(`${ulid(1)}:214`);
  });
  it("PRD-07 opens the tracker only on request and reports an invalid address", async () => {
    const bridge = boardDay();
    const list = bridge.world.tickets[ulid(1)];
    const issue = list?.issues.find((i) => i.key === "217");
    if (!issue) throw new Error("missing ticket fixture");
    issue.url = "javascript:alert(1)";
    const { app } = renderTickets(bridge);
    await screen.findByRole("article");
    expect(bridge.openedUrls).toEqual([]);
    await userEvent.keyboard("b");
    expect(bridge.openedUrls).toEqual(["https://github.com/FurkanCodes/muster/issues/214"]);
    await userEvent.click(ticket("217"));
    await userEvent.keyboard("b");
    await waitFor(() => expect(app.ui.getState().note?.text).toContain("Only http and https"));
    expect(bridge.openedUrls).toHaveLength(1);
  });
  it("assigns with Tab and arrows, remembers the agent and opens its session", async () => {
    const { bridge, app } = renderTickets();
    await screen.findByRole("article");
    await userEvent.click(ticket("217"));
    await userEvent.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAccessibleName("Assign an agent to #217");
    expect(within(dialog).queryByRole("radio", { name: /Shell/ })).toBeNull();
    await userEvent.keyboard("{Tab}{ArrowRight}{Enter}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(bridge.sent).toContainEqual({
      StartFromIssue: {
        project: ulid(1),
        key: "217",
        harness: "codex",
        size: { rows: 24, cols: 80 },
        another: false,
        on: ulid(11),
      },
    });
    expect(app.ui.getState().screen).toBe("all");
    expect(app.ui.getState().terminal).toBe(bridge.world.sessions.at(-1)?.id);
    await userEvent.keyboard("a");
    const again = await screen.findByRole("dialog");
    expect(within(again).getByRole("radio", { name: /Codex/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
  it("chooses a shared ticket's project and offers only prompt-taking added agents", async () => {
    const bridge = boardDay();
    bridge.world.preferences.agents = [
      { name: "reader", label: "Reader", mark: "R", colour: null, takes_prompt: false, asks: true },
      {
        name: "builder",
        label: "Builder",
        mark: "B",
        colour: null,
        takes_prompt: true,
        asks: false,
      },
    ];
    renderTickets(bridge);
    await screen.findByRole("article");
    await userEvent.click(screen.getByRole("option", { name: /^KEP-28 / }));
    await userEvent.keyboard("{Enter}");
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByRole("radio", { name: /Reader/ })).toBeNull();
    await userEvent.click(within(dialog).getByRole("radio", { name: /Builder/ }));
    await userEvent.keyboard("{ArrowDown}{Enter}");
    await waitFor(() =>
      expect(
        bridge.sent.some(
          (c) =>
            typeof c !== "string" &&
            "StartFromIssue" in c &&
            c.StartFromIssue.project === ulid(4) &&
            c.StartFromIssue.harness === "builder",
        ),
      ).toBe(true),
    );
  });
  it("never closes until confirmed; then waits for session ends and asks about dirty files", async () => {
    const { bridge } = renderTickets();
    await screen.findByRole("article");
    await userEvent.keyboard("x");
    let dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByRole("button", { name: "Keep it open" })).toHaveFocus();
    expect(bridge.sent.some((c) => typeof c !== "string" && "CloseIssue" in c)).toBe(false);
    await userEvent.click(within(dialog).getByRole("button", { name: "Close ticket" }));
    dialog = await screen.findByRole("dialog", { name: "Closed #214. Clean up after it?" });
    bridge.hold("StopSession");
    await userEvent.click(within(dialog).getByRole("button", { name: "Clean up" }));
    await screen.findByRole("dialog", { name: "Cleaning up after #214…" });
    expect(bridge.sent.some((c) => typeof c !== "string" && "RemoveWorktree" in c)).toBe(false);
    act(() => bridge.release("StopSession"));
    dialog = await screen.findByRole("dialog", {
      name: "Remove ⎇ muster/issue-214-desktop-app-sessions-list-and-one-live?",
    });
    expect(within(dialog).getByRole("button", { name: "Keep it" })).toHaveFocus();
    expect(bridge.world.workspaces.some((w) => w.id === ulid(11))).toBe(true);
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove with its changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(bridge.sent).toContainEqual({ RemoveWorktree: { workspace: ulid(11), force: true } });
  });
  it("ARC-25 fetches no tickets while hidden and does not poll", async () => {
    const { app, bridge } = renderTickets();
    await screen.findByRole("article");
    const before = bridge.sent.length;
    act(() => app.actions.setVisible(false));
    await userEvent.keyboard("r");
    expect(bridge.sent).toHaveLength(before);
  });
});
