// Settings' asks of muster and the Mac, and where each agent is installed
// (PRD §6 Settings, ARC-24, ARC-25): Restart in place on General, Open
// System Settings on Notifications, Edit settings.toml and each agent's
// path and version on Agents, asked of the daemon only as the page is shown
// and on the person's word, and never of a daemon too old to answer.

import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { AppContext, type App as AppValue } from "../../app/context";
import { installKeys } from "../../app/keys";
import { MockBridge } from "../../ipc/mock/bridge";
import { workingDay } from "../../ipc/mock/scenario";
import { createDaemon } from "../../state/daemon";
import { createUi } from "../../state/ui";
import Screen from "./Settings";

function renderSettings(bridge = new MockBridge(workingDay())) {
  const { store, actions } = createUi();
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  actions.show("settings");
  void bridge.open((message) => app.daemon.apply(message));
  const keys = installKeys(app);
  render(
    <AppContext value={app}>
      <Screen />
    </AppContext>,
  );
  return { app, bridge, keys };
}

const agentsAsked = (bridge: MockBridge) => bridge.sent.filter((sent) => sent === "Agents").length;

function agentRow(name: string): HTMLElement {
  const found = screen
    .getAllByRole("generic")
    .find(
      (element) =>
        element.classList.contains("mu-agentrow") &&
        element.querySelector("b")?.textContent === name,
    );
  if (!found) throw new Error(`no row for ${name}`);
  return found;
}

describe("PRD §6 Settings › Agents, as the daemon finds each", () => {
  it("says where each agent is and its version, how to install one not found, and asks again on r", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings(bridge);
    await screen.findByRole("heading", { name: "General" });
    expect(agentsAsked(bridge)).toBe(0);
    act(() => app.actions.select("settings", "agents"));
    await waitFor(() => expect(agentRow("Claude Code")).toHaveTextContent("2.1.303"));
    expect(agentRow("Claude Code")).toHaveTextContent("✓ /opt/homebrew/bin/claude");
    expect(agentRow("Gemini CLI")).toHaveTextContent("~/.npm-global/bin/gemini");
    expect(agentRow("Copilot CLI")).toHaveTextContent(
      "✗ not foundInstall it with npm install -g @github/copilot, then read again.",
    );
    expect(agentsAsked(bridge)).toBe(1);

    bridge.settings.installed.copilot = { path: "/opt/homebrew/bin/copilot", version: "0.0.339" };
    await userEvent.keyboard("r");
    await waitFor(() => expect(agentRow("Copilot CLI")).toHaveTextContent("0.0.339"));
    expect(agentsAsked(bridge)).toBe(2);
    keys();
  });

  it("says where each is installed is not known to a daemon older than 45, and never asks it", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings(bridge);
    await screen.findByRole("heading", { name: "General" });
    act(() => bridge.setLink({ state: "up", version: "0.4.0", protocol: 44 }));
    act(() => app.actions.select("settings", "agents"));
    await waitFor(() =>
      expect(agentRow("Claude Code")).toHaveTextContent(
        "where it is installed is not known: Muster's daemon is older than this app",
      ),
    );
    expect(agentsAsked(bridge)).toBe(0);
    expect(screen.queryByText("not found")).toBeNull();
    keys();
  });

  it("opens settings.toml in the person's editor from its button, and says so", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings(bridge);
    await screen.findByRole("heading", { name: "General" });
    act(() => app.actions.select("settings", "agents"));
    await userEvent.click(await screen.findByRole("button", { name: "Edit settings.toml" }));
    await waitFor(() => expect(bridge.asked).toEqual(["edit settings.toml"]));
    expect(app.ui.getState().note?.text).toMatch(/^Opened .*settings\.toml in your editor\./);
    keys();
  });
});

describe("PRD §6 Settings' asks of muster and the Mac", () => {
  it("restarts the daemon in place on General, from its row's ⏎ or its button, and says what muster said", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings(bridge);
    await screen.findByRole("button", { name: "Restart in place" });
    act(() => app.actions.select("settings", "general/restart"));
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(app.ui.getState().note?.text).toBe(
        "The daemon is now muster 0.4.1, its sessions running on.",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Restart in place" }));
    await waitFor(() => expect(bridge.asked).toEqual(["restart in place", "restart in place"]));
    keys();
  });

  it("opens the Mac's notification settings for Muster from Notifications", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings(bridge);
    await screen.findByRole("heading", { name: "General" });
    act(() => app.actions.select("settings", "notifications/system_settings"));
    const page = await screen.findByRole("heading", { name: "Notifications" });
    expect(page).toBeInTheDocument();
    const open = await screen.findByRole("button", { name: "Open System Settings" });
    await userEvent.click(open);
    await waitFor(() => expect(bridge.asked).toEqual(["notification settings"]));
    expect(app.ui.getState().note?.text).toBe("Opened System Settings at Muster's notifications.");
    expect(
      within(open.closest(".mu-setting") as HTMLElement).getByText(/allowed|not yet/),
    ).toBeTruthy();
    keys();
  });
});
