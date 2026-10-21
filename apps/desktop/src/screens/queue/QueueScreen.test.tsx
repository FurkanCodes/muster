import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockBridge } from "../../ipc/mock/bridge";
import { ulid, workingDay } from "../../ipc/mock/scenario";
import { renderApp } from "../../test/render";

const queue = () => screen.getByRole("listbox");
const options = () => within(queue()).getAllByRole("option");
const selected = () => within(queue()).getByRole("option", { selected: true });
const names = () => options().map((option) => option.getAttribute("aria-label"));
const hints = () => screen.getByRole("contentinfo").textContent;

async function opened(bridge = new MockBridge(workingDay())) {
  const view = renderApp(bridge);
  await screen.findByRole("option", { name: "Sketch the Tauri shell" });
  return view;
}

describe("ARC-24 Needs you, as the Triage board draws it", () => {
  it("lists who asks with their question, then finishes not seen, then agents at work", async () => {
    await opened();
    const labels = within(queue())
      .getAllByRole("presentation")
      .map((label) => label.textContent);
    expect(labels).toEqual(["Asking you", "Finished, not seen", "Working"]);
    expect(names()).toEqual([
      "Sketch the Tauri shell",
      "Share the Gradle cache across worktrees",
      "Resize settles once on release",
      "Explain the release script",
      "Split view.rs by responsibility",
      "Flaky resize test under load",
      "Upgrade AGP to 9.1",
      "Sign-up page loads the old pricing",
    ]);
    const asking = selected();
    expect(asking).toHaveClass("mu-item");
    expect(asking).toHaveTextContent("⎇ feature/desktop-app in muster");
    expect(within(asking).getByText("wants to run cargo build")).toHaveClass("mu-question-ask");
    expect(asking).toHaveTextContent("⏎answer");
    // Only the selected item shows its keys.
    expect(options()[1]).not.toHaveTextContent("⏎answer");
    const finish = within(queue()).getByRole("option", { name: "Resize settles once on release" });
    expect(finish).toHaveClass("mu-row", "mu-row--two");
    expect(finish).toHaveTextContent("⎇ fix/resize-under-load+64−22#88approved✓12m");
    expect(within(finish).getByRole("img", { name: "not seen" })).toBeInTheDocument();
    const plain = within(queue()).getByRole("option", { name: "Explain the release script" });
    expect(plain).toHaveTextContent("no changes");
    const worker = within(queue()).getByRole("option", { name: "Upgrade AGP to 9.1" });
    expect(worker).toHaveClass("mu-row");
    expect(worker).not.toHaveClass("mu-row--two");
    expect(worker).toHaveTextContent("running ./gradlew assembleDebug8m");
  });

  it("says All clear, with how many work, when nothing asks or waits unseen", async () => {
    const { bridge } = await opened();
    act(() => {
      for (const id of [101, 200]) bridge.setStatus(ulid(id), { state: "running" });
      for (const id of [104, 202]) bridge.setStatus(ulid(id), { state: "finished" });
    });
    expect(await screen.findByRole("heading", { name: "All clear" })).toBeInTheDocument();
    expect(
      screen.getByText("6 agents at work. Muster will show them here when they need you."),
    ).toBeInTheDocument();
    expect(within(queue()).getByText("Working")).toBeInTheDocument();
  });

  it("tells the daemon out of reach, keeping the queue as it last stood", async () => {
    const { bridge } = await opened();
    act(() => bridge.setLink({ state: "down", reason: "the daemon stopped", since: 0 }));
    // The window's banner says it, with the daemon's reason (src/flows/Away.tsx).
    expect(screen.getByRole("alert")).toHaveTextContent("Muster's daemon is out of reach.");
    expect(screen.getByRole("alert")).toHaveTextContent("the daemon stopped");
    expect(selected()).toHaveAccessibleName("Sketch the Tauri shell");
  });

  it("says it is connecting before the daemon's first word", () => {
    const bridge = new MockBridge(workingDay());
    bridge.open = async () => {};
    renderApp(bridge);
    return waitFor(() => expect(screen.getByText("Connecting to Muster…")).toBeInTheDocument());
  });
});

describe("ARC-24 Not seen and Working are the queue filtered", () => {
  it("shows finishes not seen on Not seen, and agents at work on two lines on Working", async () => {
    await opened();
    await userEvent.click(screen.getByRole("button", { name: /^Not seen/ }));
    await waitFor(() =>
      expect(names()).toEqual(["Resize settles once on release", "Explain the release script"]),
    );
    expect(screen.getByRole("heading", { name: "Not seen" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Working/ }));
    await waitFor(() => expect(options()).toHaveLength(4));
    const worker = within(queue()).getByRole("option", { name: "Upgrade AGP to 9.1" });
    expect(worker).toHaveClass("mu-row--two");
    expect(worker).toHaveTextContent("running ./gradlew assembleDebuglamphaus8m");
    expect(within(queue()).queryByRole("presentation")).not.toBeInTheDocument();
  });

  it("narrows each to the project chosen, with what each says when empty", async () => {
    await opened();
    await userEvent.click(screen.getByRole("button", { name: /^lamphaus/ }));
    await waitFor(() =>
      expect(names()).toEqual([
        "Share the Gradle cache across worktrees",
        "Explain the release script",
        "Upgrade AGP to 9.1",
      ]),
    );
    await userEvent.click(screen.getByRole("button", { name: /^lamphaus/ }));
    await userEvent.click(screen.getByRole("button", { name: /^kep-site/ }));
    expect(await screen.findByRole("heading", { name: "All clear" })).toBeInTheDocument();
    expect(
      screen.getByText("No agent is at work. Muster will show them here when they need you."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Not seen/ }));
    expect(await screen.findByRole("heading", { name: "All seen" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^Working/ }));
    expect(await screen.findByRole("heading", { name: "No agent at work" })).toBeInTheDocument();
  });
});

describe("ARC-24 the queue's keys", () => {
  it("moves with ↑↓ and j k, the pane following, and says each state's keys", async () => {
    await opened();
    expect(hints()).toContain("⏎answer");
    expect(hints()).toContain("zput back");
    await userEvent.keyboard("{ArrowDown}");
    expect(selected()).toHaveAccessibleName("Share the Gradle cache across worktrees");
    expect(
      screen.getByRole("heading", { level: 2, name: "Share the Gradle cache across worktrees" }),
    ).toBeInTheDocument();
    await userEvent.keyboard("j");
    expect(selected()).toHaveAccessibleName("Resize settles once on release");
    expect(hints()).toContain("⏎open");
    expect(hints()).toContain("bpull request");
    await userEvent.keyboard("kk{ArrowUp}");
    expect(selected()).toHaveAccessibleName("Sketch the Tauri shell");
    expect(queue()).toHaveAttribute("aria-activedescendant", selected().id);
  });

  it("gives the terminal the keys on ⏎ and takes them back on ⇧Q", async () => {
    const { container } = await opened();
    await userEvent.keyboard("{Enter}");
    expect(selected()).toHaveClass("is-resting");
    expect(hints()).toContain("⇧Qgive the keys back");
    expect(hints()).toContain("Typing in Sketch the Tauri shell. Every key goes to Codex.");
    // Keys go to the program now, not the list.
    await userEvent.keyboard("{ArrowDown}");
    expect(selected()).toHaveAccessibleName("Sketch the Tauri shell");
    await userEvent.keyboard("Q");
    expect(selected()).not.toHaveClass("is-resting");
    expect(container.querySelector(".has-keys")).toBeNull();
  });

  it("holds a selected finish in place once seen, until another is selected", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("jj");
    expect(selected()).toHaveAccessibleName("Resize settles once on release");
    act(() => {
      bridge.updateSession(ulid(104), { unseen: false });
      bridge.emitSession(ulid(104));
    });
    expect(selected()).toHaveAccessibleName("Resize settles once on release");
    expect(names()[2]).toBe("Resize settles once on release");
    await userEvent.keyboard("j");
    expect(selected()).toHaveAccessibleName("Explain the release script");
    expect(names()).not.toContain("Resize settles once on release");
  });

  it("gives a session's place to the next as it leaves", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("j");
    act(() => bridge.removeSession(ulid(200)));
    expect(selected()).toHaveAccessibleName("Resize settles once on release");
  });

  it("goes back to the first that waits after answering one, as the terminal UI does", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("j{Enter}");
    act(() => bridge.setStatus(ulid(200), { state: "running" }));
    expect(selected()).toHaveAccessibleName("Share the Gradle cache across worktrees");
    await userEvent.keyboard("Q");
    expect(selected()).toHaveAccessibleName("Sketch the Tauri shell");
  });

  it("l goes to the session that needed the person last, here or on All sessions", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("l");
    expect(hints()).toContain("No session has needed you since Muster started.");
    act(() => bridge.setStatus(ulid(201), { state: "needs_you", reason: "question" }));
    await userEvent.keyboard("l");
    expect(selected()).toHaveAccessibleName("Upgrade AGP to 9.1");
    act(() => bridge.setStatus(ulid(201), { state: "finished" }));
    await userEvent.keyboard("k");
    await userEvent.keyboard("l");
    expect(await screen.findByRole("heading", { name: "All sessions" })).toBeInTheDocument();
  });

  it("PRD-14 d shows the changes; z puts the newest turn back, never while its agent works", async () => {
    const { bridge } = await opened();
    act(() => bridge.setStatus(ulid(101), { state: "running" }));
    await userEvent.keyboard("z");
    expect(hints()).toContain("Wait for its turn to end.");
    act(() => bridge.setStatus(ulid(101), { state: "needs_you", reason: "permission" }));
    await userEvent.keyboard("z");
    const changes = screen.getByRole("button", { name: "Changes" });
    expect(changes).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(changes);
    expect(changes).toHaveAttribute("aria-pressed", "false");
    await userEvent.keyboard("d");
    expect(changes).toHaveAttribute("aria-pressed", "true");
    // A workspace with no turn kept has none to put back.
    await userEvent.keyboard("jj");
    await userEvent.keyboard("z");
    expect(hints()).toContain("No turn is kept for this workspace yet.");
  });
});

describe("ARC-24 renaming, closing and the pull request from the list", () => {
  it("R renames in place: ⏎ keeps the name, ⎋ leaves it as it was", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("R");
    const field = screen.getByRole("textbox", { name: "Session name" });
    expect(field).toHaveFocus();
    expect(field).toHaveValue("Sketch the Tauri shell");
    await userEvent.clear(field);
    await userEvent.type(field, "The Tauri shell{Enter}");
    expect(await screen.findByRole("option", { name: "The Tauri shell" })).toBeInTheDocument();
    expect(bridge.sent).toContainEqual({
      RenameSession: { session: ulid(101), title: "The Tauri shell" },
    });
    expect(hints()).toContain("Renamed.");
    await userEvent.keyboard("R");
    await userEvent.type(screen.getByRole("textbox", { name: "Session name" }), " again{Escape}");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(selected()).toHaveAccessibleName("The Tauri shell");
  });

  it("keeps the field, saying why, when the name is refused", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.failNext(
      { RenameSession: { session: ulid(101), title: "Refused" } },
      "the store is read-only",
    );
    await opened(bridge);
    await userEvent.keyboard("R");
    const field = screen.getByRole("textbox", { name: "Session name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Refused{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("the store is read-only");
    expect(field).toBeInvalid();
  });

  it("PRD-09 x asks first; Keep it does nothing; Close session stops it, then takes it off", async () => {
    const { bridge } = await opened();
    await userEvent.keyboard("x");
    const dialog = await screen.findByRole("dialog", { name: "Close Sketch the Tauri shell?" });
    expect(dialog).toHaveTextContent("Its agent stops and it leaves the list.");
    expect(within(dialog).getByRole("button", { name: "Keep it" })).toHaveFocus();
    // The queue behind it hears none of the dialog's keys.
    await userEvent.keyboard("j");
    expect(selected()).toHaveAccessibleName("Sketch the Tauri shell");
    await userEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(bridge.sent).not.toContainEqual({ StopSession: { session: ulid(101) } });
    await userEvent.keyboard("x");
    await userEvent.click(screen.getByRole("button", { name: "Close session" }));
    expect(bridge.sent).toContainEqual({ StopSession: { session: ulid(101) } });
    await waitFor(() =>
      expect(bridge.sent).toContainEqual({ RemoveSession: { session: ulid(101) } }),
    );
    await waitFor(() => expect(names()).not.toContain("Sketch the Tauri shell"));
    expect(hints()).toContain("Closed the session.");
    expect(selected()).toHaveAccessibleName("Share the Gradle cache across worktrees");
  });

  it("removes an ended session at once, without asking", async () => {
    const { bridge } = await opened();
    act(() => {
      bridge.updateSession(ulid(104), { ended_at: 0 });
      bridge.setStatus(ulid(104), { state: "exited", success: false }, true);
    });
    await userEvent.click(
      within(queue()).getByRole("option", { name: "Resize settles once on release" }),
    );
    expect(selected()).toHaveAccessibleName("Resize settles once on release");
    await userEvent.keyboard("x");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(names()).not.toContain("Resize settles once on release"));
    expect(hints()).toContain("Removed the session.");
  });

  it("says the daemon's refusal to stop, and removes nothing", async () => {
    const bridge = new MockBridge(workingDay());
    bridge.failNext({ StopSession: { session: ulid(101) } }, "the session would not stop");
    await opened(bridge);
    await userEvent.keyboard("x");
    // ⏎ on the dialog, not on one of its answers, closes the session.
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Enter" });
    expect(await screen.findByText("the session would not stop")).toBeInTheDocument();
    expect(bridge.sent).not.toContainEqual({ RemoveSession: { session: ulid(101) } });
  });

  it("b says opening the pull request in the browser is coming, and where it is", async () => {
    await opened();
    await userEvent.keyboard("b");
    expect(hints()).toContain(
      "Opening pull requests in the browser is coming to the app. #91 is at https://github.com/FurkanCodes/muster/pull/91",
    );
    await userEvent.keyboard("j");
    await userEvent.keyboard("b");
    expect(hints()).toContain("No pull request from this branch is known.");
  });
});
