import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AppContext, type App as AppValue } from "../../app/context";
import { installKeys } from "../../app/keys";
import { BridgeFailure } from "../../ipc/bridge";
import { MockBridge } from "../../ipc/mock/bridge";
import { workingDay } from "../../ipc/mock/scenario";
import { createDaemon } from "../../state/daemon";
import { createUi } from "../../state/ui";
import Screen from "./Settings";

/** The Settings screen over a fresh working day, with the window's keys; opened unless asked not to. */
function renderSettings({ open = true, bridge = new MockBridge(workingDay()) } = {}) {
  const { store, actions } = createUi();
  const app: AppValue = { bridge, daemon: createDaemon(), ui: store, actions };
  actions.show("settings");
  if (open) void bridge.open((message) => app.daemon.apply(message));
  const keys = installKeys(app);
  const view = render(
    <AppContext value={app}>
      <Screen />
    </AppContext>,
  );
  return { app, bridge, keys, ...view };
}

function row(key: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(`[data-row="${key}"]`);
  if (!found) throw new Error(`no row ${key}`);
  return found;
}

describe("Settings", () => {
  it("says it is reading Muster's settings until the daemon sends them", async () => {
    const { keys } = renderSettings({ open: false });
    expect(
      await screen.findByRole("status", { name: "Reading Muster's own settings…" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument();
    keys();
  });

  it("reads the plugins and every project's settings as it is shown", async () => {
    const { bridge, keys } = renderSettings();
    await screen.findByRole("heading", { name: "General" });
    await waitFor(() => expect(bridge.sent).toContainEqual({ Plugin: "List" }));
    await waitFor(() =>
      expect(
        bridge.sent.filter(
          (command) => typeof command === "object" && "ProjectSettings" in command,
        ),
      ).toHaveLength(4),
    );
    keys();
  });

  it("walks every setting with ↑↓, page to page, and the sections follow", async () => {
    const { keys } = renderSettings();
    await screen.findByRole("group", { name: "Theme" });
    expect(row("general/theme")).toHaveClass("is-selected");
    for (let at = 0; at < 7; at += 1) await userEvent.keyboard("{ArrowDown}");
    expect(row("notifications/notifications")).toHaveClass("is-selected");
    expect(screen.getByRole("button", { name: "Notifications" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await userEvent.keyboard("k");
    expect(row("general/restart")).toHaveClass("is-selected");
    keys();
  });

  it("turns the selected switch with ⏎, through the daemon, and says so", async () => {
    const { app, bridge, keys } = renderSettings();
    await screen.findByRole("group", { name: "Theme" });
    act(() => app.actions.select("settings", "notifications/notch"));
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(app.ui.getState().note?.text).toBe("Muster leaves the notch alone."),
    );
    expect(bridge.sent).toContainEqual({ ChangePreference: { Notch: false } });
    expect(screen.getByRole("switch", { name: "Agents in the notch" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
    keys();
  });

  it("says a refusal as the daemon wrote it", async () => {
    const bridge = new MockBridge(workingDay());
    const { app, keys } = renderSettings({ bridge });
    await screen.findByRole("group", { name: "Theme" });
    const answer = bridge.command.bind(bridge);
    vi.spyOn(bridge, "command").mockImplementation((command) =>
      typeof command === "object" && "ChangePreference" in command
        ? Promise.reject(
            new BridgeFailure(
              "refused",
              "~/.config/muster/settings.toml cannot be written: read-only",
              "bad_request",
            ),
          )
        : answer(command),
    );
    await userEvent.keyboard("{ArrowDown}{ArrowRight}");
    await waitFor(() =>
      expect(app.ui.getState().note).toEqual({
        text: "~/.config/muster/settings.toml cannot be written: read-only",
        error: true,
      }),
    );
    keys();
  });

  it("turns the shared Rust builds off with ⏎", async () => {
    const { app, bridge, keys } = renderSettings();
    await screen.findByRole("group", { name: "Theme" });
    expect(screen.getByRole("switch", { name: "Shared Rust builds" })).toBeChecked();
    act(() => app.actions.select("settings", "general/shared_builds"));
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({ ChangePreference: { SharedBuilds: false } }),
    );
    keys();
  });

  it("hears a sound stepped to with ← →, and not the Mac's own alert", async () => {
    const { app, bridge, keys } = renderSettings();
    await screen.findByRole("group", { name: "Theme" });
    act(() => app.actions.select("settings", "notifications/tell.error"));
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(bridge.heard).toEqual(["Blow"]));
    act(() => app.actions.select("settings", "notifications/tell.finish"));
    await userEvent.keyboard("{ArrowLeft}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        ChangePreference: { Tell: { kind: "finish", tell: { on: true, sound: "Tink" } } },
      }),
    );
    expect(bridge.heard).toEqual(["Blow", "Tink"]);
    keys();
  });

  it("shows the person's keys in its hints, and answers them", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.preferences.keys = { read_settings: ["f5"] };
    const { app, keys } = renderSettings({ bridge });
    await screen.findByRole("group", { name: "Theme" });
    await waitFor(() =>
      expect(app.ui.getState().hints).toContainEqual({ key: "F5", does: "read the files again" }),
    );
    await userEvent.keyboard("r");
    expect(bridge.sent).not.toContain("ReadPreferences");
    await userEvent.keyboard("{F5}");
    await waitFor(() => expect(bridge.sent).toContain("ReadPreferences"));
    keys();
  });

  it("goes back to Sessions on the back key", async () => {
    const { app, keys } = renderSettings();
    await screen.findByRole("group", { name: "Theme" });
    await userEvent.keyboard("{Escape}");
    expect(app.ui.getState().screen).toBe("all");
    keys();
  });
});
