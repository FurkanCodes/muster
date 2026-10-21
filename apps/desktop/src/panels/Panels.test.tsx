// The panels in the session pane: the agent alone, the shell on T and a
// plugin's panel on its key beside it, each put away by its key or its x,
// the line between two resized and kept in the person's settings, a layout
// read back from them, and full screen on f (ARC-24, ARC-25, PRD-15, PRD §6).

import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { installKeys } from "../app/keys";
import { ulid } from "../ipc/mock/scenario";
import { SessionPane } from "../session/SessionPane";
import { renderWithApp } from "../test/render";

const codex = ulid(101);
const gitLog = ulid(102);
let uninstall = () => {};
afterEach(() => uninstall());

async function pane() {
  const rendered = renderWithApp(<SessionPane session={codex} />);
  uninstall = installKeys(rendered.app);
  await screen.findByRole("heading", { name: "Sketch the Tauri shell" });
  return rendered;
}

const terminals = () => screen.getAllByLabelText("Terminal").map((one) => one.dataset.session);

describe("ARC-24 the panels beside the agent", () => {
  it("show the agent's terminal alone, framed as a terminal, while nothing stands beside it", async () => {
    await pane();
    await screen.findByLabelText("Terminal");
    expect(terminals()).toEqual([codex]);
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Codex" })).not.toBeInTheDocument();
  });

  it("open the workspace's shell beside it on T, with the keys, and put it away on T", async () => {
    const { app } = await pane();
    await userEvent.keyboard("T");
    const shell = await screen.findByRole("region", { name: "Shell" });
    expect(shell).toHaveTextContent("Shell");
    expect(shell).toHaveTextContent("git log");
    expect(shell).toHaveClass("has-keys");
    expect(screen.getByRole("region", { name: "Codex" })).toHaveTextContent(
      "wants to run cargo build",
    );
    await waitFor(() => expect(terminals()).toEqual([codex, gitLog]));
    expect(screen.getByRole("button", { name: "Shell" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("separator")).toHaveAccessibleName("Between the agent and the shell");
    // The keys back with Muster, T puts it away; it runs on.
    act(() => app.actions.keysTo("list"));
    await userEvent.keyboard("T");
    expect(screen.queryByRole("region", { name: "Shell" })).not.toBeInTheDocument();
    expect(app.ui.getState().note?.text).toBe("The shell is put away; it runs on.");
    expect(screen.getByRole("button", { name: "Shell" })).toHaveAttribute("aria-pressed", "false");
  });

  it("put the shell away by the x on its head, and take it away as it ends", async () => {
    const { app, bridge } = await pane();
    await userEvent.click(screen.getByRole("button", { name: "Shell" }));
    const shell = await screen.findByRole("region", { name: "Shell" });
    await userEvent.click(within(shell).getByRole("button", { name: "Put the shell away" }));
    expect(screen.queryByRole("region", { name: "Shell" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Shell" }));
    await screen.findByRole("region", { name: "Shell" });
    act(() => bridge.setStatus(gitLog, { state: "exited", success: true }));
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Shell" })).not.toBeInTheDocument(),
    );
    expect(app.ui.getState()).toMatchObject({ shell: null, keysIn: "list" });
  });

  it("open a plugin's panel on its key beside the session, and put it away on its key (PRD-15)", async () => {
    const { bridge } = await pane();
    await userEvent.keyboard("G");
    const graph = await screen.findByRole("region", { name: "Session graph" });
    expect(graph).toHaveTextContent("from Zoetrope");
    expect(within(graph).getByRole("button", { name: "Put Session graph away" })).toHaveAttribute(
      "title",
      "Put it away (G)",
    );
    await userEvent.keyboard("G");
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Session graph" })).not.toBeInTheDocument(),
    );
    expect(
      bridge.sent.some((command) => typeof command === "object" && "RemoveSession" in command),
    ).toBe(true);
  });

  it("resize by the line between them, kept in the person's settings", async () => {
    const { app, bridge } = await pane();
    await userEvent.keyboard("T");
    act(() => app.actions.keysTo("list"));
    const line = await screen.findByRole("separator");
    line.focus();
    fireEvent.keyDown(line, { key: "ArrowRight" });
    await waitFor(() =>
      expect(app.daemon.store.getState().preferences?.panels).not.toBe(
        "list | session 60% | (changes / shell / plugin)",
      ),
    );
    const sent = bridge.sent.at(-1);
    expect(sent).toMatchObject({
      ChangePreference: { Panels: expect.stringContaining("session") },
    });
    // The arrows were the line's: the list beside heard none of them.
    expect(app.ui.getState().selected).toEqual({});
  });

  it("read the layout from the person's settings", async () => {
    const { bridge } = await pane();
    act(() =>
      bridge.publish({
        preferences: {
          ...bridge.world.preferences,
          panels: "list | (session / shell) | changes | plugin",
        },
      }),
    );
    await userEvent.keyboard("T");
    expect(await screen.findByRole("separator")).toHaveAttribute("aria-orientation", "horizontal");
  });

  it("say the daemon's refusal to keep a layout, and put the kept one back", async () => {
    const { app, bridge } = await pane();
    await userEvent.keyboard("T");
    act(() => app.actions.keysTo("list"));
    bridge.failNext("ChangePreference", "Muster's own settings: cannot write", "bad_request");
    const line = await screen.findByRole("separator");
    fireEvent.keyDown(line, { key: "ArrowLeft" });
    await waitFor(() =>
      expect(app.ui.getState().note).toEqual({
        text: "Muster's own settings: cannot write",
        error: true,
      }),
    );
    expect(line).toHaveAttribute("aria-valuenow");
  });
});

describe("ARC-24 full screen", () => {
  it("fills the window with the session on f, with the keys; Esc gives the window back", async () => {
    const { app } = await pane();
    await userEvent.keyboard("f");
    expect(app.ui.getState()).toMatchObject({
      fullScreen: codex,
      keysIn: "terminal",
      terminal: codex,
    });
    act(() => app.actions.keysTo("list"));
    await userEvent.keyboard("{Escape}");
    expect(app.ui.getState().fullScreen).toBeNull();
    await userEvent.keyboard("f");
    act(() => app.actions.keysTo("list"));
    await userEvent.keyboard("f");
    expect(app.ui.getState().fullScreen).toBeNull();
  });

  it("ends with the session shown, and on going to another screen", async () => {
    const { app, unmount } = await pane();
    await userEvent.click(screen.getByRole("button", { name: "Full screen" }));
    expect(app.ui.getState().fullScreen).toBe(codex);
    act(() => app.actions.show("tickets"));
    expect(app.ui.getState().fullScreen).toBeNull();
    act(() => app.actions.fillWindow(codex));
    unmount();
    expect(app.ui.getState().fullScreen).toBeNull();
  });
});
