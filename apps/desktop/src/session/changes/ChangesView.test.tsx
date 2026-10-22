import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installKeys } from "../../app/keys";
import type { Command } from "../../ipc/generated/Command";
import type { FileDiff } from "../../ipc/generated/FileDiff";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderWithApp } from "../../test/render";
import ChangesView from "./ChangesView";

const workspace = ulid(11);
let removeKeys: (() => void) | undefined;
beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      disconnect() {}
    },
  );
  Element.prototype.scrollIntoView = vi.fn();
  Element.prototype.scrollTo = vi.fn();
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  removeKeys?.();
  vi.unstubAllGlobals();
});

function setup(bridge = new MockBridge(workingDay())) {
  const result = renderWithApp(<ChangesView workspace={workspace} />, bridge);
  removeKeys = installKeys(result.app);
  act(() => result.app.actions.showPane("changes"));
  return result;
}

/** What the daemon would answer for src/link.rs, not committed. */
function linkDiff(bridge: MockBridge, diff: FileDiff) {
  bridge.world.diffs ??= {};
  bridge.world.diffs[`${workspace}:uncommitted:src/link.rs`] = diff;
}

function sent(bridge: MockBridge, name: string): Command[] {
  return bridge.sent.filter((command) => typeof command !== "string" && name in command);
}

describe("ARC-24 Not committed", () => {
  it("lists each file's kind, path and lines, and the whole in the bar", async () => {
    setup();
    const files = await screen.findByRole("listbox", { name: "Files not committed" });
    const link = within(files).getByRole("option", { name: "modified: src/link.rs" });
    expect(link).toHaveTextContent("Msrc/link.rs+118−4");
    expect(link).toHaveAttribute("aria-selected", "true");
    expect(within(files).getByRole("option", { name: "untracked: Cargo.toml" })).toHaveTextContent(
      "?Cargo.toml+24",
    );
    expect(screen.getByRole("button", { name: "Not committed 3" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "On the branch 2" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Turns 4" })).toBeVisible();
    expect(screen.getByText("in 3 files").parentElement).toHaveTextContent("+203−4in 3 files");
    expect(screen.getByText("Choose a file to see its diff.")).toBeVisible();
    expect(screen.getByText(/Muster only asks git/)).toBeVisible();
  });

  it("⏎ or a click opens the file's diff; ⎋ in it goes back to the list, ⎋ there to the terminal", async () => {
    const { app, bridge } = setup();
    await userEvent.click(await screen.findByRole("option", { name: "modified: src/link.rs" }));
    const diff = await screen.findByRole("region", { name: "Diff" });
    expect(diff).toHaveFocus();
    expect(diff.querySelector(".mu-dl--add")).toHaveTextContent(
      "+use muster_client::{Connection, Received};",
    );
    expect(diff.querySelector(".mu-dl--del")).toHaveTextContent("2−use muster_client::Connection;");
    expect(diff.querySelector(".mu-dl-hunk")).toHaveTextContent("@@ -1,2 +1,3 @@");
    const head = diff.parentElement?.querySelector(".mu-diffview-head");
    expect(head).toHaveTextContent("Msrc/link.rs+118−4Not committed");
    expect(sent(bridge, "FileDiff")).toEqual([
      { FileDiff: { workspace, part: "uncommitted", path: "src/link.rs" } },
    ]);
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("option", { name: "modified: src/link.rs" })).toHaveFocus();
    expect(app.ui.getState().pane).toBe("changes");
    await userEvent.keyboard("j{Enter}");
    await waitFor(() =>
      expect(sent(bridge, "FileDiff")).toContainEqual({
        FileDiff: { workspace, part: "uncommitted", path: "src/main.rs" },
      }),
    );
    await userEvent.keyboard("{Escape}h");
    expect(screen.getByText("Choose a file to see its diff.")).toBeVisible();
    await userEvent.keyboard("{Escape}");
    expect(app.ui.getState().pane).toBe("terminal");
  });

  it("names the folder all the files are in once, above, as the board does", async () => {
    const bridge = new MockBridge(workingDay());
    const changes = bridge.world.changes?.[workspace];
    if (changes)
      changes.uncommitted = changes.uncommitted.map((file) => ({
        ...file,
        path: `crates/muster-desktop/${file.path}`,
      }));
    setup(bridge);
    const row = await screen.findByRole("option", {
      name: "modified: crates/muster-desktop/src/link.rs",
    });
    expect(row).toHaveTextContent("Msrc/link.rs");
    expect(screen.getByText(/Muster only asks git/)).toHaveTextContent(
      "In crates/muster-desktop. Muster only asks git; nothing here changes the worktree.",
    );
  });

  it("says a binary file in words", async () => {
    const bridge = new MockBridge(workingDay());
    const changes = bridge.world.changes?.[workspace];
    if (changes)
      changes.uncommitted = [{ path: "logo.png", from: null, kind: "added", lines: null }];
    setup(bridge);
    expect(await screen.findByRole("option", { name: "added: logo.png" })).toHaveTextContent(
      "Alogo.pngbinary",
    );
  });
});

describe("ARC-24 On the branch", () => {
  it("lists the commits since the base, then the files they changed, ⏎ asking the branch's diff", async () => {
    const { bridge } = setup();
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    await userEvent.keyboard("{Tab}");
    expect(screen.getByRole("heading", { name: "Commits since main · 1" })).toBeVisible();
    expect(screen.getByText("Add the desktop crate").parentElement).toHaveTextContent(
      "a1b2c3d Add the desktop crate",
    );
    expect(screen.getByRole("heading", { name: "Changed on the branch · 2 files" })).toBeVisible();
    await userEvent.keyboard("j{Enter}");
    await waitFor(() =>
      expect(sent(bridge, "FileDiff")).toEqual([
        { FileDiff: { workspace, part: "branch", path: "Cargo.toml" } },
      ]),
    );
    expect(await screen.findByText("On the branch")).toBeVisible();
  });

  it.each([
    [
      null,
      "No branch to measure its commits against: set base_branch in muster.toml, or fetch the remote.",
    ],
    ["origin/main", "Nothing has changed here since origin/main."],
  ])("with base %s and no commits says so", async (base, words) => {
    const bridge = new MockBridge(workingDay());
    bridge.world.changes ??= {};
    bridge.world.changes[workspace] = {
      base,
      uncommitted: [],
      branch: [],
      commits: [],
      more_commits: 0,
    };
    setup(bridge);
    expect(await screen.findByText("Nothing here is waiting to be committed.")).toBeVisible();
    await userEvent.keyboard("{Tab}");
    expect(screen.getByText(words)).toBeVisible();
  });
});

describe("ARC-24 Turns", () => {
  it("lists each entry with its agent, what it changed and when; ⏎ asks its diff", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.timelines?.[workspace]?.entries.unshift(
      {
        number: 6,
        kind: { kind: "moved", onto: "main" },
        started: 0,
        ended: 0,
        summary: null,
        head_before: null,
        head_after: null,
      },
      {
        number: 5,
        kind: { kind: "undo", of: 2 },
        started: 0,
        ended: 0,
        summary: null,
        head_before: null,
        head_after: null,
      },
    );
    setup(bridge);
    await userEvent.click(await screen.findByRole("button", { name: /^Turns/ }));
    const turns = screen.getByRole("listbox", { name: "Turns" });
    const rows = within(turns).getAllByRole("option");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("6⎇Moved onto mainchanged no files"),
      expect.stringContaining("5↺Put back as before #2changed no files"),
      "4Codex, turn 33 files +203−41m",
      "3✎Edits between turns1 file +25m",
      "2Codex, turn 22 files +40−425m",
      "1Codex, turn 11 file +12−440m",
    ]);
    expect(within(rows[2] as HTMLElement).getByRole("img", { name: "Codex" })).toBeVisible();
    expect(screen.getByText("Muster keeps each workspace's newest 100.")).toBeVisible();
    await userEvent.keyboard("jj{Enter}");
    await waitFor(() =>
      expect(sent(bridge, "EntryDiff")).toEqual([{ EntryDiff: { workspace, number: 4 } }]),
    );
    const diff = await screen.findByRole("region", { name: "Diff" });
    expect(diff.parentElement?.querySelector(".mu-diffview-head")).toHaveTextContent(
      "Codex, turn 3+203−4",
    );
  });

  it("z asks what putting the selected turn back would do; elsewhere it says to choose a turn", async () => {
    const day = new MockBridge(workingDay());
    // Its agent has finished, so nothing works in the workspace.
    for (const session of day.world.sessions)
      if (session.workspace === workspace) session.status = { state: "finished" };
    const { app, bridge } = setup(day);
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    await userEvent.keyboard("z");
    expect(app.ui.getState().note).toEqual({
      text: "Choose a turn under Turns to put the files back as they were before it.",
      error: true,
    });
    await userEvent.keyboard("{Tab}{Tab}");
    await screen.findByRole("listbox", { name: "Turns" });
    // A diff open on turn 4, then the keys back in the list on turn 2.
    await userEvent.keyboard("{Enter}");
    await screen.findByRole("region", { name: "Diff" });
    await userEvent.keyboard("{Escape}jj");
    await userEvent.keyboard("z");
    expect(
      await screen.findByRole("dialog", { name: "Put the files back as before Codex's turn 2?" }),
    ).toBeVisible();
    expect(sent(bridge, "UndoPlan")).toEqual([{ UndoPlan: { workspace, number: 2 } }]);
    // Keys belong to the dialog while it is open.
    await userEvent.keyboard("y");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sent(bridge, "Undo")).toEqual([{ Undo: { workspace, number: 2, move_branch: false } }]);
    // The new entry is selected, and the listing and timeline are asked again.
    await waitFor(() =>
      expect(screen.getByRole("option", { selected: true })).toHaveTextContent(
        "Put back as before #2",
      ),
    );
    expect(app.ui.getState().keysIn).toBe("list");
  });

  it("the Put back button does what z does", async () => {
    const { bridge } = setup();
    await userEvent.click(await screen.findByRole("button", { name: /^Turns/ }));
    await screen.findByRole("listbox", { name: "Turns" });
    await userEvent.click(screen.getByRole("button", { name: "Put back" }));
    await screen.findByRole("dialog");
    expect(sent(bridge, "UndoPlan")).toEqual([{ UndoPlan: { workspace, number: 4 } }]);
  });

  it.each([
    ["put-back", "UndoPlan"],
    ["diff", "EntryDiff"],
  ] as const)("the session pane's This turn opens it on a turn (%s)", async (intent, command) => {
    const { app, bridge } = setup();
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    act(() => app.actions.showTurn(workspace, 4, intent));
    await waitFor(() =>
      expect(sent(bridge, command)).toEqual([{ [command]: { workspace, number: 4 } }]),
    );
    expect(screen.getByRole("button", { name: "Turns 4" })).toHaveAttribute("aria-pressed", "true");
    expect(app.ui.getState().pane).toBe("changes");
  });
});

describe("ARC-24 asking, refused and empty", () => {
  it.each([
    ["WorkspaceChanges", "Asking git…", "", "Nothing here is waiting to be committed."],
    [
      "Timeline",
      "Reading the turns…",
      "The timeline could not be read: ",
      "None kept yet: each agent's turn here is, from now on.",
    ],
  ])("%s: asking, then refused, then asked again", async (command, asking, lead, empty) => {
    const bridge = new MockBridge(workingDay());
    bridge.hold(command);
    setup(bridge);
    if (command === "Timeline")
      await userEvent.click(await screen.findByRole("button", { name: /^Turns/ }));
    expect(await screen.findByText(asking)).toHaveAttribute("role", "status");
    await act(async () => {
      bridge.failNext(command, "git could not read the worktree", "internal");
      bridge.release(command);
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      `${lead}git could not read the worktree`,
    );
    bridge.world.changes ??= {};
    bridge.world.changes[workspace] = {
      base: "main",
      uncommitted: [],
      branch: [],
      commits: [],
      more_commits: 0,
    };
    bridge.world.timelines ??= {};
    bridge.world.timelines[workspace] = { entries: [] };
    await userEvent.click(screen.getByRole("button", { name: "Ask again" }));
    expect(await screen.findByText(empty)).toBeVisible();
  });
});

describe("ARC-25 asked again only when something moved", () => {
  it("follows the newest entry and the summary, as the TUI's settle_changes, never while hidden", async () => {
    const { app, bridge } = setup();
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    await waitFor(() => expect(sent(bridge, "Timeline")).toHaveLength(1));
    const listings = () => sent(bridge, "WorkspaceChanges").length;
    const timelines = () => sent(bridge, "Timeline").length;
    expect(listings()).toBe(1);
    // Another workspace moving asks nothing.
    act(() => bridge.updateWorkspace(ulid(12), { timeline: 20, changes: null }));
    // The same summary the listing comes to asks nothing.
    act(() => bridge.updateWorkspace(workspace, { changes: { files: 3, added: 300, removed: 6 } }));
    expect([listings(), timelines()]).toEqual([1, 1]);
    // A turn ended: the timeline is asked again, and the summary that moved, the listing.
    act(() =>
      bridge.updateWorkspace(workspace, {
        timeline: 5,
        changes: { files: 1, added: 1, removed: 0 },
      }),
    );
    await waitFor(() => expect([listings(), timelines()]).toEqual([2, 2]));
    // The listing's answer brings the summary back in step: asked no more.
    await act(async () => {});
    expect(listings()).toBe(2);
    // Hidden, nothing is asked; seen again, only what moved meanwhile.
    act(() => app.actions.setVisible(false));
    act(() => bridge.updateWorkspace(workspace, { timeline: 6 }));
    expect(timelines()).toBe(2);
    act(() => app.actions.setVisible(true));
    await waitFor(() => expect(timelines()).toBe(3));
    expect(listings()).toBe(2);
    // r asks git again.
    await userEvent.keyboard("r");
    await waitFor(() => expect([listings(), timelines()]).toEqual([3, 4]));
  });
});

describe("ARC-24 diffs said in words", () => {
  it("binary, no text, cut and refused", async () => {
    const { bridge } = setup();
    linkDiff(bridge, {
      text: "diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ",
      cut: false,
    });
    await userEvent.click(await screen.findByRole("option", { name: "modified: src/link.rs" }));
    expect(await screen.findByText("binary: git shows no lines for it.")).toBeVisible();
    linkDiff(bridge, { text: "", cut: true });
    await userEvent.keyboard("r");
    expect(await screen.findByText("Git shows no difference in its text.")).toBeVisible();
    expect(screen.getByText("… cut here: the rest is longer than Muster shows.")).toBeVisible();
    bridge.failNext("FileDiff", "src/link.rs is no longer changed there", "not_found");
    await userEvent.keyboard("r");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "src/link.rs is no longer changed there r asks again.",
    );
  });

  it("ARC-25 draws fewer than 100 of 5,000 lines, and the last ones once scrolled there", async () => {
    const { bridge, container } = setup();
    linkDiff(bridge, {
      text: `@@ -0,0 +1,5000 @@\n${Array.from({ length: 5000 }, (_, n) => `+line ${n}`).join("\n")}`,
      cut: false,
    });
    const start = performance.now();
    await userEvent.click(await screen.findByRole("option", { name: "modified: src/link.rs" }));
    const diff = await screen.findByRole("region", { name: "Diff" });
    expect(performance.now() - start).toBeLessThan(1000);
    expect(container.querySelectorAll(".mu-dl").length).toBeLessThan(100);
    fireEvent.scroll(diff, { target: { scrollTop: 89_900 } });
    expect(within(diff).getByText("line 4999")).toBeVisible();
    expect(container.querySelectorAll(".mu-dl").length).toBeLessThan(100);
  });
});

describe("ARC-24 other work on the same files", () => {
  it.each([
    [
      false,
      "Overlap with ⎇ fix/resize-under-load: both change src/a.rs and 4 more. Git could merge them.",
    ],
    [true, "Conflict with ⎇ fix/resize-under-load: would conflict in src/a.rs and 3 more."],
  ])(
    "says it as the TUI does (conflict=%s), and 1 shows the other's changes",
    async (conflict, words) => {
      const { app, bridge } = setup();
      expect((await screen.findByText("Overlap")).closest(".mu-banner")).toHaveTextContent(
        "Overlap with ⎇ fix/resize-under-load: both change crates/muster-client/src/connection.rs. Git could merge them.",
      );
      act(() =>
        bridge.updateWorkspace(workspace, {
          clashes: [
            {
              with: ulid(12),
              files: 5,
              conflicts: conflict ? 4 : 0,
              shown: [{ path: "src/a.rs", conflict }],
            },
          ],
        }),
      );
      const banner = screen.getByText(conflict ? "Conflict" : "Overlap").closest(".mu-banner");
      // The mark is drawn by the banner's tone, never a glyph in its text.
      expect(banner).toHaveTextContent(`${words}See its changes`);
      expect(banner).toHaveClass(conflict ? "mu-banner--waiting" : "mu-banner--overlap");
      await userEvent.keyboard("1");
      expect(app.ui.getState().selected.all).toBe(ulid(103));
      expect(app.ui.getState().screen).toBe("all");
      expect(app.ui.getState().pane).toBe("changes");
    },
  );

  it("says so where nothing runs in the other workspace, and goes when the clash does", async () => {
    const { app, bridge } = setup();
    await screen.findByText("Overlap");
    act(() =>
      bridge.updateWorkspace(workspace, {
        clashes: [
          { with: ulid(30), files: 1, conflicts: 0, shown: [{ path: "a", conflict: false }] },
        ],
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "See its changes" }));
    expect(app.ui.getState().note?.text).toBe(
      "Nothing runs in ⌂ main, so it has no pane to show its changes in.",
    );
    act(() => bridge.updateWorkspace(workspace, { clashes: [] }));
    expect(screen.queryByText("Overlap")).toBeNull();
  });
});

describe("ARC-24 the changes' actions answer the person's keys", () => {
  it("asks again, puts back, lists the keys and goes back on the keys set, and hints them", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.world.preferences.keys = {
      changes_again: ["g"],
      put_back: ["u"],
      close_changes: ["x"],
      changes_help: ["ctrl-h"],
    };
    const { app } = setup(bridge);
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    await waitFor(() =>
      expect(app.ui.getState().hints).toContainEqual({ key: "g", does: "ask git again" }),
    );
    expect(app.ui.getState().hints).toContainEqual({ key: "x", does: "back to the terminal" });
    const listings = () => sent(bridge, "WorkspaceChanges").length;
    expect(listings()).toBe(1);
    await userEvent.keyboard("r");
    expect(listings()).toBe(1);
    await userEvent.keyboard("g");
    await waitFor(() => expect(listings()).toBe(2));

    await userEvent.click(screen.getByRole("button", { name: /^Turns/ }));
    expect(app.ui.getState().hints).toContainEqual({ key: "u", does: "put back" });
    await screen.findByRole("option", { name: /Codex, turn 3/ });
    await userEvent.keyboard("z");
    expect(sent(bridge, "UndoPlan")).toEqual([]);
    await userEvent.keyboard("u");
    await waitFor(() => expect(sent(bridge, "UndoPlan")).toHaveLength(1));
    await userEvent.click(await screen.findByRole("button", { name: "Keep it" }));

    await userEvent.keyboard("{Control>}h{/Control}");
    expect(app.ui.getState()).toMatchObject({ paletteOpen: true, paletteMode: "changes" });
    act(() => app.actions.openPalette(false));
    await userEvent.keyboard("d");
    expect(app.ui.getState().pane).toBe("changes");
    await userEvent.keyboard("x");
    expect(app.ui.getState().pane).toBe("terminal");
  });

  it("⇧Q, as in a session, goes back to the terminal; ? lists the changes' keys", async () => {
    const { app } = setup();
    await screen.findByRole("option", { name: "modified: src/link.rs" });
    await userEvent.keyboard("?");
    expect(app.ui.getState()).toMatchObject({ paletteOpen: true, paletteMode: "changes" });
    act(() => app.actions.openPalette(false));
    await userEvent.keyboard("Q");
    expect(app.ui.getState().pane).toBe("terminal");
  });
});
