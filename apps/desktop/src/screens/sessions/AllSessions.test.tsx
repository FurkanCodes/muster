// All sessions as a component: a session selected for the person unfolds
// what hides it, the status bar says each line's keys, a gone line leaves
// the selection on the one in its place, and the list's keys rename and
// close as on the queue (ARC-24, PRD §6).

import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../../app/keys";
import { ulid } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import AllSessions from "./AllSessions";

const lamphaus = ulid(2);
const gradle = ulid(200);
let uninstall = () => {};
beforeEach(() => localStorage.clear());
afterEach(() => uninstall());

async function screenShown() {
  const rendered = renderWithApp(<AllSessions />);
  uninstall = installKeys(rendered.app);
  await screen.findByRole("tree");
  return rendered;
}

/** A line of the tree, by its whole name or a pattern. */
const row = (name: string | RegExp) =>
  screen.getByRole("treeitem", {
    name: typeof name === "string" ? (accessible: string) => accessible === name : name,
  });

describe("ARC-24 All sessions", () => {
  it("selects the first session at first, and shows its pane", async () => {
    const { app } = await screenShown();
    await waitFor(() => expect(app.ui.getState().selected.all).toBe(ulid(100)));
    expect(row(/^Split view\.rs/)).toHaveAttribute("aria-selected", "true");
    expect(
      await screen.findByRole("heading", { name: "Split view.rs by responsibility" }),
    ).toBeInTheDocument();
  });

  it("unfolds what hides a session selected for the person", async () => {
    const { app } = await screenShown();
    await userEvent.click(row("lamphaus"));
    await userEvent.keyboard("e");
    expect(row("lamphaus")).toHaveAttribute("aria-expanded", "false");
    act(() => app.actions.select("all", gradle));
    await waitFor(() => expect(row("lamphaus")).toHaveAttribute("aria-expanded", "true"));
    expect(row(/^Share the Gradle cache/)).toHaveAttribute("aria-selected", "true");
    expect(JSON.parse(localStorage.getItem("muster.sessions.folds") ?? "{}").projects).toEqual({});
  });

  it("says each line's keys in the status bar", async () => {
    const { app } = await screenShown();
    await waitFor(() =>
      expect(app.ui.getState().hints.map((hint) => hint.does)).toEqual([
        "type in it",
        "shell",
        "full screen",
        "see changes",
        "rename",
        "close",
      ]),
    );
    await userEvent.click(row("lamphaus"));
    expect(app.ui.getState().hints).toEqual([
      { key: "⏎", does: "fold or unfold" },
      { key: "⇧T", does: "shell" },
    ]);
    act(() => app.actions.keysTo("terminal"));
    expect(app.ui.getState().hints[0]).toEqual({ key: "⇧Q", does: "give the keys back" });
  });

  it("names the keys as the person set them", async () => {
    const { app, bridge } = await screenShown();
    act(() =>
      bridge.publish({
        preferences: { ...bridge.world.preferences, keys: { shell: ["S"], fold: ["z"] } },
      }),
    );
    await userEvent.click(row("lamphaus"));
    expect(app.ui.getState().hints[1]).toEqual({ key: "⇧S", does: "shell" });
    await userEvent.keyboard("z");
    expect(row("lamphaus")).toHaveAttribute("aria-expanded", "false");
  });

  it("leaves the selection on the line in its place when the selected one goes", async () => {
    const { app, bridge } = await screenShown();
    await userEvent.click(row(/^Resize settles/));
    act(() => bridge.removeSession(ulid(104)));
    await waitFor(() => expect(app.ui.getState().selected.all).not.toBe(ulid(104)));
    expect(screen.getAllByRole("treeitem", { selected: true })).toHaveLength(1);
  });

  it("renames a chat in place on R, and asks before closing one that runs on x", async () => {
    const { bridge } = await screenShown();
    await userEvent.click(row(/^Split view\.rs/));
    await userEvent.keyboard("R");
    const field = await screen.findByRole("textbox", { name: "Session name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Rows apart{Enter}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        RenameSession: { session: ulid(100), title: "Rows apart" },
      }),
    );
    await userEvent.keyboard("x");
    expect(await screen.findByRole("dialog", { name: "Close Rows apart?" })).toBeInTheDocument();
  });

  it("narrows to the project the window is narrowed to", async () => {
    const { app } = await screenShown();
    act(() => app.actions.narrow(lamphaus));
    await waitFor(() => expect(screen.queryByRole("treeitem", { name: "muster" })).toBeNull());
    expect(row("lamphaus")).toBeInTheDocument();
  });
});
