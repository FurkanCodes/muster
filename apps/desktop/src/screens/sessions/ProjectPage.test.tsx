// A project's page, beside the All list while its line is selected: where
// its work is, each place with its branch and sessions, and a box that
// starts work in it, the prompt first (PRD §6a).

import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installKeys } from "../../app/keys";
import { ulid } from "../../ipc/mock/scenario";
import { renderList } from "../../test/render";

let uninstall = () => {};
beforeEach(() => localStorage.clear());
afterEach(() => uninstall());

/** The All list with muster's line selected, its page beside it. */
async function musterPage() {
  const rendered = renderList("all");
  uninstall = installKeys(rendered.app);
  await screen.findByRole("tree");
  await userEvent.click(
    screen.getByRole("treeitem", { name: (accessible) => accessible === "muster" }),
  );
  const page = await screen.findByRole("region", { name: "muster" });
  return { ...rendered, page };
}

describe("PRD §6a a project's page", () => {
  it("shows where its work is: its checkout and each worktree, with their sessions", async () => {
    const { page } = await musterPage();
    expect(within(page).getByRole("heading", { level: 2, name: "muster" })).toBeInTheDocument();
    expect(within(page).getByText("your checkout")).toBeInTheDocument();
    expect(within(page).getByText("⎇ feature/desktop-app")).toBeInTheDocument();
    expect(within(page).getByText("⎇ fix/resize-under-load")).toBeInTheDocument();
    // A session there is a click from its terminal.
    await userEvent.click(within(page).getByRole("button", { name: /^Flaky resize test/ }));
    expect(
      await screen.findByRole("heading", { level: 2, name: "Flaky resize test under load" }),
    ).toBeInTheDocument();
  });

  it("starts work from its box: a new worktree, then the agent with the prompt", async () => {
    const { page, bridge, app } = await musterPage();
    const box = within(page).getByRole("form", { name: "Start work" });
    await userEvent.type(within(box).getByRole("textbox", { name: "First prompt" }), "Fix it");
    // The window's own keys stay out of a prompt being typed.
    expect(app.ui.getState().selected.all).toBe(ulid(1));
    await userEvent.keyboard("{Meta>}{Enter}{/Meta}");
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        CreateWorktree: { project: ulid(1), branch: null, on: null },
      }),
    );
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({
        StartSession: expect.objectContaining({ harness: "claude", prompt: "Fix it" }),
      }),
    );
    // Then the session, its terminal with the keys.
    await waitFor(() => expect(app.ui.getState().keysIn).toBe("terminal"));
  });

  it("+ on a place starts work there, saying who already works there", async () => {
    const { page } = await musterPage();
    expect(within(page).queryByRole("status")).toBeNull();
    await userEvent.click(
      within(page).getByRole("button", { name: "Start work in feature/desktop-app" }),
    );
    expect(within(page).getByRole("textbox", { name: "First prompt" })).toHaveFocus();
    expect(within(page).getByRole("status")).toHaveTextContent(
      "Sketch the Tauri shell already works here",
    );
  });
});
