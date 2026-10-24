// The panels in the session pane: the agent alone, the shell on T and a
// plugin's panel on its key beside it, each put away by its key or its x,
// the line between two resized and kept in the person's settings, a layout
// read back from them, and full screen on f (ARC-24, ARC-25, PRD-15, PRD §6).

import { act, createEvent, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { AppContext } from "../app/context";
import { installKeys } from "../app/keys";
import { ulid } from "../ipc/mock/scenario";
import { SessionActionsProvider } from "../session/SessionActionsContext";
import { SessionPane } from "../session/SessionPane";
import { renderWithApp } from "../test/render";
import { tooltipOf } from "../test/tooltip";
import { CARRIED, carry } from "./carry";

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
    expect(graph).toHaveTextContent("graph, from the plugin");
    expect(await tooltipOf(within(graph).getByRole("button", { name: "Put the graph away" }))).toBe(
      "Put it away (⇧G)",
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

describe("ARC-24 moving the panels with the keys", () => {
  it("moves the panel with the keys on m and the arrows, ⏎ keeping the layout", async () => {
    const { app, bridge } = await pane();
    await userEvent.keyboard("T");
    act(() => app.actions.keysTo("list"));
    await screen.findByRole("region", { name: "Shell" });
    // With the keys on the list, the agent is picked; ⇥ picks the shell.
    await userEvent.keyboard("m");
    expect(app.ui.getState().note?.text).toMatch(/^Moving the agent: the arrows move it/);
    await userEvent.keyboard("{Tab}");
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "horizontal");
    expect(app.ui.getState().selected).toEqual({});
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(bridge.world.preferences.panels).toBe(
        "list | ((session 60% | (changes / plugin)) / shell)",
      ),
    );
  });

  it("puts the layout back on Esc, and says when nothing stands beside to move", async () => {
    const { app, bridge } = await pane();
    await userEvent.keyboard("m");
    expect(app.ui.getState().note).toEqual({
      text: "Nothing stands beside the session to move.",
      error: true,
    });
    await userEvent.keyboard("T");
    act(() => app.actions.keysTo("list"));
    await screen.findByRole("region", { name: "Shell" });
    await userEvent.keyboard("m{Tab}{ArrowDown}");
    expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "horizontal");
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("separator")).toHaveAttribute("aria-orientation", "vertical");
    expect(
      bridge.sent.some((command) => typeof command === "object" && "ChangePreference" in command),
    ).toBe(false);
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
    await userEvent.click(screen.getByRole("button", { name: "More" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Full screen/ }));
    expect(app.ui.getState().fullScreen).toBe(codex);
    act(() => app.actions.show("tickets"));
    expect(app.ui.getState().fullScreen).toBeNull();
    act(() => app.actions.fillWindow(codex));
    unmount();
    expect(app.ui.getState().fullScreen).toBeNull();
  });

  it("pin the session shown in a panel of its own on P, drawn once, and unpin it on P again (PRD §6)", async () => {
    await pane();
    await screen.findByLabelText("Terminal");
    const box = () => document.querySelector(".panels") as HTMLElement;
    await userEvent.keyboard("P");
    expect(
      await within(box()).findByRole("region", { name: "Sketch the Tauri shell" }),
    ).toBeInTheDocument();
    // Its terminal once, in its own panel; the agent's panel, with nothing else to show, goes.
    await waitFor(() => expect(terminals()).toEqual([codex]));
    expect(screen.queryByRole("region", { name: "Codex" })).not.toBeInTheDocument();
    await userEvent.keyboard("P");
    await waitFor(() =>
      expect(
        within(box()).queryByRole("region", { name: "Sketch the Tauri shell" }),
      ).not.toBeInTheDocument(),
    );
    expect(terminals()).toEqual([codex]);
  });

  it("pin a session whose line is carried from the sidebar and dropped on the pane (PRD §6a)", async () => {
    const { app } = await pane();
    const other = ulid(100);
    const box = (await screen.findByLabelText("Terminal")).closest(".panels") as HTMLElement;
    carry(other);
    // jsdom has no DragEvent: the events are given where the pointer is.
    const at = (event: Event) => {
      Object.defineProperty(event, "clientX", { value: 1 });
      Object.defineProperty(event, "clientY", { value: 1 });
      return event;
    };
    const dataTransfer = { types: [CARRIED], dropEffect: "none" };
    fireEvent(box, at(createEvent.dragOver(box, { dataTransfer })));
    fireEvent(box, at(createEvent.drop(box, { dataTransfer })));
    await waitFor(() => expect(terminals()).toEqual(expect.arrayContaining([codex, other])));
    expect(app.ui.getState().note?.text).toBe("Pinned it in a panel of its own.");
  });

  it("keep the agent shown before beside a pinned session selected, both terminals drawn (PRD §6a)", async () => {
    const { app, rerender } = await pane();
    const other = ulid(100);
    await screen.findByLabelText("Terminal");
    const shown = (session: string) => (
      <AppContext value={app}>
        <SessionActionsProvider>
          <SessionPane session={session} />
        </SessionActionsProvider>
      </AppContext>
    );
    // The other shown as its line is taken up, then pinned: Codex stays.
    rerender(shown(other));
    await userEvent.keyboard("P");
    await waitFor(() => expect(terminals()).toEqual(expect.arrayContaining([codex, other])));
    rerender(shown(codex));
    await waitFor(() => expect(terminals()).toEqual(expect.arrayContaining([codex, other])));
    rerender(shown(other));
    await waitFor(() => expect(terminals()).toEqual(expect.arrayContaining([codex, other])));
    expect(terminals()).toHaveLength(2);
    expect(screen.queryByText(/It is in a panel of its own/)).not.toBeInTheDocument();
  });
});
