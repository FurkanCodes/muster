// The All list as a component: projects and their sessions, a session
// selected for the person unfolds what hides it, the status bar says each
// line's keys, a gone line leaves the selection on the one in its place, and
// the list's keys rename and close as on the queue (ARC-24, PRD §6a).

import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../../app/keys";
import type { Session } from "../../ipc/generated/Session";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderList } from "../../test/render";

const gradle = ulid(200);
let uninstall = () => {};
beforeEach(() => localStorage.clear());
afterEach(() => uninstall());

async function screenShown() {
  const rendered = renderList("all");
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
      { key: "n", does: "start work" },
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
    expect(app.ui.getState().hints[2]).toEqual({ key: "⇧S", does: "shell" });
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

  it("lists projects and their sessions only: ← goes from a session to its project, → back", async () => {
    const { app } = await screenShown();
    expect(screen.queryByRole("treeitem", { name: "feature/desktop-app" })).toBeNull();
    expect(screen.queryByRole("treeitem", { name: /^Shell: git log/ })).toBeNull();
    await userEvent.click(row(/^Sketch the Tauri shell/));
    await userEvent.keyboard("{ArrowLeft}");
    expect(app.ui.getState().selected.all).toBe(ulid(1));
    await userEvent.keyboard("{ArrowLeft}");
    expect(row("muster")).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    expect(row("muster")).toHaveAttribute("aria-expanded", "true");
    expect(app.ui.getState().selected.all).toBe(ulid(100));
  });

  it("shows a project's page, its box ready, where work is asked to start; once", async () => {
    const { app } = await screenShown();
    act(() => app.actions.compose(ulid(2), ulid(20)));
    const page = await screen.findByRole("region", { name: "lamphaus" });
    expect(screen.getByRole("textbox", { name: "First prompt" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Where" })).toHaveTextContent("Its checkout");
    expect(page).toBeInTheDocument();
    // Done: showing the page again leaves the keys with the list.
    expect(app.ui.getState().compose).toBeNull();
  });

  it("takes no keys while Tickets or Settings is shown beside it", async () => {
    const rendered = renderList("all", false);
    uninstall = installKeys(rendered.app);
    await screen.findByRole("tree");
    await waitFor(() => expect(rendered.app.ui.getState().selected.all).toBe(ulid(100)));
    await userEvent.keyboard("j");
    expect(rendered.app.ui.getState().selected.all).toBe(ulid(100));
    expect(screen.queryByRole("heading", { level: 2 })).toBeNull();
  });
});

describe("ARC-25 a tree of thousands of sessions", () => {
  it("draws only the lines in sight, and the one selected for the person wherever it is", async () => {
    const bridge = new MockBridge(workingDay());
    const many = Array.from({ length: 3000 }, (_, n) => ({
      ...(bridge.world.sessions[0] as Session),
      id: ulid(20_000 + n),
      title: `Session ${n}`,
    }));
    bridge.world.sessions.push(...many);
    const rendered = renderList("all", true, bridge);
    uninstall = installKeys(rendered.app);
    const tree = await screen.findByRole("tree");
    await screen.findByRole("treeitem", { name: /^Session 0,/ });
    expect(screen.getAllByRole("treeitem").length).toBeLessThan(80);
    expect(screen.queryByRole("treeitem", { name: /^Session 2999,/ })).toBeNull();

    act(() => rendered.app.actions.select("all", ulid(20_000 + 2999)));
    const last = await screen.findByRole("treeitem", { name: /^Session 2999,/ });
    expect(last).toHaveAttribute("aria-selected", "true");
    expect(tree).toHaveAttribute("aria-activedescendant", last.id);
    expect(tree.scrollTop).toBeGreaterThan(100_000);
    expect(screen.getAllByRole("treeitem").length).toBeLessThan(80);
    // The keys move on from there, drawn as they go.
    await userEvent.keyboard("k");
    expect(screen.getByRole("treeitem", { name: /^Session 2998,/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
